use rusqlite::{Connection, Result, params};
use std::{
    collections::HashMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use common::note_obj::ZNote;
// ============ 被测代码（和你项目里的一样） ============


pub fn init_db<P: AsRef<Path>>(path_db: P) -> Result<Connection> {
    let conn = Connection::open(path_db)?; // 测试用内存库
    conn.pragma_update(None, "journal_mode", "WAL")?; // 读写不互斥
    conn.pragma_update(None, "synchronous", "NORMAL")?; // 性能/安全平衡
    conn.pragma_update(None, "foreign_keys", "ON")?; // 外键约束默认是关的
    conn.pragma_update(None, "busy_timeout", 5000)?; // 锁竞争时等待 5s 而不是立
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- 便签ID，自增主键
        owner       TEXT NOT NULL,                      -- 归属人（如用户名/用户ID）
        title       TEXT,                               -- 可选：便签标题，便于列表展示
        content     TEXT NOT NULL,                      -- 长文本内容，SQLite 的 TEXT 不受长度限制
        hash        TEXT NOT NULL,  
        created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),  -- 创建时间（Unix时间戳）
        updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))   -- 最近操作时间
    );
    ",
    )?;
    Ok(conn)
}
fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// 内容哈希：用于 Diff 比对，客户端与服务器对同一内容算出的 hash 应一致
fn content_hash(content: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ---------- 增 ----------
pub fn add_note(conn: &Connection, owner: &str, title: Option<&str>, content: &str) -> Result<i64> {
    let ts = now_ts();
    conn.execute(
        "INSERT INTO notes (owner, title, content, hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![owner, title, content, content_hash(content), ts],
    )?;
    Ok(conn.last_insert_rowid())
}

// ---------- 查（单个） ----------
pub fn get_note(conn: &Connection, id: i64) -> Result<Option<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at
         FROM notes WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(ZNote {
            id: row.get(0)?,
            owner: row.get(1)?,
            title: row.get(2)?,
            hash: row.get(3)?,
            content: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        }))
    } else {
        Ok(None)
    }
}

// ---------- 查（列表，按 updated_at 倒序，最新的靠前） ----------
pub fn list_notes(conn: &Connection, owner: &str, limit: i64, offset: i64) -> Result<Vec<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at
         FROM notes
         WHERE owner = ?1
         ORDER BY updated_at DESC, id DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let notes = stmt
        .query_map(params![owner, limit, offset], |row| {
            Ok(ZNote {
                id: row.get(0)?,
                owner: row.get(1)?,
                title: row.get(2)?,
                hash: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}
// ---------- 查（列表，按 updated_at 倒序，最新的靠前） ----------
pub fn list_notes_by_name(conn: &Connection, owner: &str) -> Result<Vec<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at
         FROM notes
         WHERE owner = ?1
         ORDER BY updated_at DESC, id DESC
",
    )?;
    let notes = stmt
        .query_map(params![owner], |row| {
            Ok(ZNote {
                id: row.get(0)?,
                owner: row.get(1)?,
                title: row.get(2)?,
                hash: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}
// ---------- 改（同时刷新 hash 与 updated_at） ----------
pub fn update_note(conn: &Connection, id: i64, title: Option<&str>, content: &str) -> Result<usize> {
    conn.execute(
        "UPDATE notes SET title = ?2, content = ?3, hash = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, title, content, content_hash(content), now_ts()],
    )
}

// ---------- 删 ----------
pub fn delete_note(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
}

// ---------- 差异比对：客户端发来 (id, hash) 列表，
// 返回服务器中同 id 但 hash 不一致的笔记 id（即需要客户端推送回来的） ----------
pub fn compare_hashes(
    conn: &Connection,
    owner: &str,
    list: &[(String, String)],
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT CAST(id AS TEXT), hash FROM notes WHERE owner = ?1")?;
    let server: HashMap<String, String> = stmt
        .query_map(params![owner], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<HashMap<_, _>>>()?;

    let need = list
        .iter()
        .filter(|(id, hash)| server.get(id.as_str()).is_some_and(|h| h != hash))
        .map(|(id, _)| id.clone())
        .collect();
    Ok(need)
}

// ---------- 批量 upsert（同步推送用）：id 已存在则更新，不存在则按客户端 id 创建 ----------
pub fn upsert_notes(conn: &Connection, owner: &str, notes: &[ZNote]) -> Result<()> {
    let ts = now_ts();
    let mut stmt = conn.prepare(
        "INSERT INTO notes (id, owner, title, content, hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             content = excluded.content,
             hash = excluded.hash,
             updated_at = excluded.updated_at
         WHERE notes.owner = excluded.owner",
    )?;
    for note in notes {
        stmt.execute(params![
            note.id,
            owner,
            note.title,
            note.content,
            note.hash,
            ts
        ])?;
    }
    Ok(())
}

// ---------- 批量删除：按客户端要求删除指定 id 的笔记（仅限该 owner 名下） ----------
pub fn delete_notes(conn: &Connection, owner: &str, ids: &[String]) -> Result<usize> {
    let mut total = 0;
    let mut stmt = conn.prepare("DELETE FROM notes WHERE id = ?1 AND owner = ?2")?;
    for id in ids {
        total += stmt.execute(params![id, owner])?;
    }
    Ok(total)
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let conn = init_db("E:\\code\\tracer-mcp\\dfdf.db").unwrap();
        let id = add_note(&conn, "zjz", Some("wu"), "sdfsdfsdfsdfsdfsdf").unwrap();
        let body = get_note(&conn, id).unwrap();

        // body 是 Option<Note>，两种打印方式：
        println!("{:?}", body); // 打印整个结构（Debug 格式）

        // 只打印 content 字段（查到才打印）：
        if let Some(note) = body {
            println!("{}", note.content);
        }
    }
}
