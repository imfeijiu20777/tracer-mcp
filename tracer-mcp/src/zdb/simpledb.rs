use rusqlite::{Connection, Result, params};
use std::{path::Path, time::{SystemTime, UNIX_EPOCH}};
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

// ---------- 增 ----------
pub fn add_note(conn: &Connection, owner: &str, title: Option<&str>, content: &str) -> Result<i64> {
    let ts = now_ts();
    conn.execute(
        "INSERT INTO notes (owner, title, content, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![owner, title, content, ts],
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
                hash:row.get(3)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}

// ---------- 改（同时刷新 updated_at） ----------
pub fn update_note(conn: &Connection, id: i64, title: Option<&str>, content: &str) -> Result<usize> {
    conn.execute(
        "UPDATE notes SET title = ?2, content = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, title, content, now_ts()],
    )
}

// ---------- 删 ----------
pub fn delete_note(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id])
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
