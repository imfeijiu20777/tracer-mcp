use rusqlite::{Connection, Result, params};
use std::{
    collections::HashMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use crate::note_obj::{NoteMeta, ZNote};

pub fn init_db<P: AsRef<Path>>(path_db: P) -> Result<Connection> {
    let conn = Connection::open(path_db)?;
    conn.pragma_update(None, "journal_mode", "WAL")?; // 读写不互斥
    conn.pragma_update(None, "synchronous", "NORMAL")?; // 性能/安全平衡
    conn.pragma_update(None, "foreign_keys", "ON")?; // 外键约束默认是关的
    conn.pragma_update(None, "busy_timeout", 5000)?; // 锁竞争时等待 5s 而不是立即报错
    init_schema(&conn)?;
    Ok(conn)
}

/// 建表 + 旧库迁移（客户端/服务端共用，保证两端表结构同构）
fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- 便签ID，自增主键
        owner       TEXT NOT NULL,                      -- 归属人（如用户名/用户ID）
        title       TEXT,                               -- 可选：便签标题，便于列表展示
        content     TEXT NOT NULL,                      -- 长文本内容，SQLite 的 TEXT 不受长度限制
        hash        TEXT NOT NULL,
        created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),  -- 创建时间（Unix时间戳）
        updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),  -- 最近操作时间
        deleted     INTEGER NOT NULL DEFAULT 0          -- 墓碑标记：1 代表已删除，参与同步合并
    );
    ",
    )?;
    // 旧库迁移：缺 deleted 列时补上，存量行默认未删除
    let has_deleted = conn
        .prepare("PRAGMA table_info(notes)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?
        .iter()
        .any(|name| name == "deleted");
    if !has_deleted {
        conn.execute_batch("ALTER TABLE notes ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;")?;
    }
    Ok(())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// 内容哈希：覆盖 title+content，元数据 (updated_at, hash, created_at, deleted)
// 相等即语义相等，Sync 阶段一只传元数据即可做合并判定
fn content_hash(title: Option<&str>, content: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    title.hash(&mut h);
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

// 统一行映射，列顺序：id, owner, title, hash, content, created_at, updated_at, deleted
fn row_to_note(row: &rusqlite::Row<'_>) -> Result<ZNote> {
    Ok(ZNote {
        id: row.get(0)?,
        owner: row.get(1)?,
        title: row.get(2)?,
        hash: row.get(3)?,
        content: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        deleted: row.get::<_, i64>(7)? != 0,
    })
}

// ---------- 增 ----------
pub fn add_note(conn: &Connection, owner: &str, title: Option<&str>, content: &str) -> Result<i64> {
    let ts = now_ts();
    conn.execute(
        "INSERT INTO notes (owner, title, content, hash, created_at, updated_at, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0)",
        params![owner, title, content, content_hash(title, content), ts],
    )?;
    Ok(conn.last_insert_rowid())
}

// ---------- 查（单个） ----------
pub fn get_note(conn: &Connection, id: i64) -> Result<Option<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at, deleted
         FROM notes WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row_to_note(&row)?))
    } else {
        Ok(None)
    }
}

// ---------- 查（列表，按 updated_at 倒序，最新的靠前；不含墓碑） ----------
pub fn list_notes(conn: &Connection, owner: &str, limit: i64, offset: i64) -> Result<Vec<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at, deleted
         FROM notes
         WHERE owner = ?1 AND deleted = 0
         ORDER BY updated_at DESC, id DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let notes = stmt
        .query_map(params![owner, limit, offset], row_to_note)?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}

// ---------- 查（全量列表，按 updated_at 倒序；不含墓碑，UI 用） ----------
pub fn list_notes_by_name(conn: &Connection, owner: &str) -> Result<Vec<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at, deleted
         FROM notes
         WHERE owner = ?1 AND deleted = 0
         ORDER BY updated_at DESC, id DESC
",
    )?;
    let notes = stmt
        .query_map(params![owner], row_to_note)?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}

// ---------- 查（全量含墓碑，Sync 专用） ----------
pub fn list_all_notes(conn: &Connection, owner: &str) -> Result<Vec<ZNote>> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, title,hash, content, created_at, updated_at, deleted
         FROM notes
         WHERE owner = ?1
         ORDER BY updated_at DESC, id DESC
",
    )?;
    let notes = stmt
        .query_map(params![owner], row_to_note)?
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}

// ---------- 查（全量元数据含墓碑，Sync 阶段一专用） ----------
pub fn list_all_metas(conn: &Connection, owner: &str) -> Result<Vec<NoteMeta>> {
    let mut stmt = conn.prepare(
        "SELECT id, created_at, updated_at, hash, deleted
         FROM notes
         WHERE owner = ?1",
    )?;
    let metas = stmt
        .query_map(params![owner], |row| {
            Ok(NoteMeta {
                id: row.get(0)?,
                created_at: row.get(1)?,
                updated_at: row.get(2)?,
                hash: row.get(3)?,
                deleted: row.get::<_, i64>(4)? != 0,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(metas)
}

// ---------- 改（同时刷新 hash 与 updated_at，置 deleted=0 复活墓碑） ----------
pub fn update_note(conn: &Connection, id: i64, title: Option<&str>, content: &str) -> Result<usize> {
    conn.execute(
        "UPDATE notes SET title = ?2, content = ?3, hash = ?4, updated_at = ?5, deleted = 0 WHERE id = ?1",
        params![id, title, content, content_hash(title, content), now_ts()],
    )
}

// ---------- 删（墓碑：置 deleted=1 并刷新 updated_at，删除状态随同步传播） ----------
pub fn delete_note(conn: &Connection, owner: &str, id: i64) -> Result<usize> {
    conn.execute(
        "UPDATE notes SET deleted = 1, updated_at = ?3 WHERE id = ?1 AND owner = ?2",
        params![id, owner, now_ts()],
    )
}

// ---------- 批量 upsert（同步写库用）：id 已存在则更新，不存在则按显式 id 创建；
// 保留传入的 created_at/updated_at，不能统一写 now，否则破坏 LWW 时间戳 ----------
pub fn upsert_notes(conn: &Connection, owner: &str, notes: &[ZNote]) -> Result<()> {
    let ts = now_ts();
    let mut stmt = conn.prepare(
        "INSERT INTO notes (id, owner, title, content, hash, created_at, updated_at, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             content = excluded.content,
             hash = excluded.hash,
             updated_at = excluded.updated_at,
             deleted = excluded.deleted
         WHERE notes.owner = excluded.owner",
    )?;
    for note in notes {
        stmt.execute(params![
            note.id,
            owner,
            note.title,
            note.content,
            note.hash,
            note.created_at.unwrap_or(ts),
            note.updated_at.unwrap_or(ts),
            note.deleted,
        ])?;
    }
    Ok(())
}

// LWW 合并键：updated_at 为主（最后写入 wins），hash 已覆盖 title+content，
// 再比 created_at/deleted 做确定性 tie-break，全序保证交换律
fn meta_key(m: &NoteMeta) -> (Option<i64>, &str, Option<i64>, bool) {
    (m.updated_at, m.hash.as_str(), m.created_at, m.deleted)
}

// ---------- 元数据双向合并：按 id 取并集，同 id 取 meta_key 字典序较大者 ----------
pub fn merge_metas(a: &[NoteMeta], b: &[NoteMeta]) -> Vec<NoteMeta> {
    let mut merged: HashMap<i64, NoteMeta> = HashMap::new();
    for meta in a.iter().chain(b.iter()) {
        merged
            .entry(meta.id)
            .and_modify(|cur| {
                if meta_key(meta) > meta_key(cur) {
                    *cur = meta.clone();
                }
            })
            .or_insert_with(|| meta.clone());
    }
    let mut out: Vec<NoteMeta> = merged.into_values().collect();
    out.sort_by_key(|m| m.id);
    out
}

// ---------- 墓碑应用：本地有行则置 deleted=1 并刷新 updated_at；
// 本地无行（对端删除了本地从未有过的笔记）则插入墓碑行，保证后续同步继续传播 ----------
pub fn apply_tombstones(conn: &Connection, owner: &str, tombs: &[NoteMeta]) -> Result<()> {
    let ts = now_ts();
    let mut stmt = conn.prepare(
        "INSERT INTO notes (id, owner, title, content, hash, created_at, updated_at, deleted)
         VALUES (?1, ?2, NULL, '', ?3, ?4, ?5, 1)
         ON CONFLICT(id) DO UPDATE SET
             deleted = 1,
             updated_at = excluded.updated_at
         WHERE notes.owner = excluded.owner",
    )?;
    for m in tombs {
        stmt.execute(params![
            m.id,
            owner,
            m.hash,
            m.created_at.unwrap_or(ts),
            m.updated_at.unwrap_or(ts),
        ])?;
    }
    Ok(())
}

/// 阶段一合并计划：need_push=客户端赢家且活着的 id（服务器等客户端补内容）；
/// give=服务器赢家且活着的全量行；tombstones=合并后的墓碑赢家（零内容传播）
pub struct SyncPlan {
    pub need_push: Vec<i64>,
    pub give: Vec<ZNote>,
    pub tombstones: Vec<NoteMeta>,
}

// ---------- 服务端阶段一：与客户端全量元数据合并，墓碑立即落库，
// 返回差异计划；两端一致的 id 不产生任何传输 ----------
pub fn sync_phase1(conn: &Connection, owner: &str, client: &[NoteMeta]) -> Result<SyncPlan> {
    let server = list_all_metas(conn, owner)?;
    let smap: HashMap<i64, &NoteMeta> = server.iter().map(|m| (m.id, m)).collect();
    let cmap: HashMap<i64, &NoteMeta> = client.iter().map(|m| (m.id, m)).collect();
    let merged = merge_metas(&server, client);

    let mut plan = SyncPlan {
        need_push: Vec::new(),
        give: Vec::new(),
        tombstones: Vec::new(),
    };
    for m in merged {
        let s = smap.get(&m.id);
        let c = cmap.get(&m.id);
        // 两端语义一致：无需传输
        if let (Some(s), Some(c)) = (s, c) {
            if meta_key(s) == meta_key(c) {
                continue;
            }
        }
        if m.deleted {
            plan.tombstones.push(m);
            continue;
        }
        let server_wins = match (s, c) {
            (Some(s), Some(c)) => meta_key(s) > meta_key(c),
            (Some(_), None) => true,
            _ => false,
        };
        if server_wins {
            if let Some(note) = get_note(conn, m.id)? {
                plan.give.push(note);
            }
        } else {
            plan.need_push.push(m.id);
        }
    }
    apply_tombstones(conn, owner, &plan.tombstones)?;
    Ok(plan)
}

// ---------- 客户端应用同步结果：upsert 服务器赢家差异行 + 应用墓碑赢家 ----------
pub fn apply_sync_result(
    conn: &Connection,
    owner: &str,
    give: &[ZNote],
    tombstones: &[NoteMeta],
) -> Result<()> {
    upsert_notes(conn, owner, give)?;
    apply_tombstones(conn, owner, tombstones)
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: i64, content: &str, updated_at: i64, deleted: bool) -> ZNote {
        ZNote {
            id,
            owner: "zjz".into(),
            title: None,
            content: content.into(),
            created_at: Some(1),
            hash: content_hash(None, content),
            updated_at: Some(updated_at),
            deleted,
        }
    }

    fn metas_of(conn: &Connection, owner: &str) -> Vec<NoteMeta> {
        list_all_notes(conn, owner)
            .unwrap()
            .iter()
            .map(NoteMeta::from)
            .collect()
    }

    #[test]
    fn test_insert_and_get() {
        let conn = init_db(":memory:").unwrap();
        let id = add_note(&conn, "zjz", Some("wu"), "sdfsdfsdfsdfsdfsdf").unwrap();
        let body = get_note(&conn, id).unwrap();
        assert!(body.is_some_and(|n| n.content == "sdfsdfsdfsdfsdfsdf" && !n.deleted));
    }

    #[test]
    fn test_old_schema_migration() {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch(
            "CREATE TABLE notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                owner TEXT NOT NULL,
                title TEXT,
                content TEXT NOT NULL,
                hash TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO notes (owner, content, hash) VALUES ('zjz', 'old', 'h');",
        )
        .unwrap();
        init_schema(&conn).unwrap();
        let all = list_all_notes(&conn, "zjz").unwrap();
        assert_eq!(all.len(), 1);
        assert!(!all[0].deleted);
    }

    #[test]
    fn test_tombstone_delete_and_revive() {
        let conn = init_db(":memory:").unwrap();
        let id = add_note(&conn, "zjz", None, "a").unwrap();
        delete_note(&conn, "zjz", id).unwrap();
        // 墓碑对 UI 不可见，但全量（Sync）可见
        assert!(list_notes_by_name(&conn, "zjz").unwrap().is_empty());
        assert_eq!(list_all_notes(&conn, "zjz").unwrap().len(), 1);
        // 更新复活墓碑
        update_note(&conn, id, None, "b").unwrap();
        assert_eq!(list_notes_by_name(&conn, "zjz").unwrap().len(), 1);
    }

    #[test]
    fn test_merge_lww() {
        // 较新更新赢
        let a = vec![NoteMeta::from(&note(1, "old", 10, false))];
        let b = vec![NoteMeta::from(&note(1, "new", 20, false))];
        assert_eq!(merge_metas(&a, &b)[0].hash, content_hash(None, "new"));
        // 较新删除赢
        let a = vec![NoteMeta::from(&note(1, "old", 10, false))];
        let b = vec![NoteMeta::from(&note(1, "old", 20, true))];
        assert!(merge_metas(&a, &b)[0].deleted);
        // 较新更新复活旧删除
        let a = vec![NoteMeta::from(&note(1, "old", 10, true))];
        let b = vec![NoteMeta::from(&note(1, "new", 20, false))];
        assert!(!merge_metas(&a, &b)[0].deleted);
        // 同秒 tie-break 满足交换律
        let a = vec![NoteMeta::from(&note(1, "aaa", 10, false))];
        let b = vec![NoteMeta::from(&note(1, "bbb", 10, false))];
        assert_eq!(merge_metas(&a, &b), merge_metas(&b, &a));
    }

    #[test]
    fn test_no_transfer_when_equal() {
        let server = init_db(":memory:").unwrap();
        let client = init_db(":memory:").unwrap();
        let rows = vec![note(1, "a", 10, false), note(2, "b", 10, true)];
        upsert_notes(&server, "zjz", &rows).unwrap();
        upsert_notes(&client, "zjz", &rows).unwrap();
        let plan = sync_phase1(&server, "zjz", &metas_of(&client, "zjz")).unwrap();
        assert!(plan.need_push.is_empty());
        assert!(plan.give.is_empty());
        assert!(plan.tombstones.is_empty());
    }

    #[test]
    fn test_sync_convergence() {
        let server = init_db(":memory:").unwrap();
        let client = init_db(":memory:").unwrap();
        // 两端各有新增，且 id=1 冲突（客户端更新）
        upsert_notes(
            &server,
            "zjz",
            &[note(1, "server-old", 10, false), note(2, "s", 10, false)],
        )
        .unwrap();
        upsert_notes(
            &client,
            "zjz",
            &[note(1, "client-new", 20, false), note(3, "c", 10, false)],
        )
        .unwrap();

        // 阶段一：客户端报元数据，服务器出计划
        let client_all = list_all_notes(&client, "zjz").unwrap();
        let plan = sync_phase1(&server, "zjz", &metas_of(&client, "zjz")).unwrap();
        assert_eq!(plan.need_push, vec![1, 3]);
        assert_eq!(plan.give.iter().map(|n| n.id).collect::<Vec<_>>(), vec![2]);
        assert!(plan.tombstones.is_empty());

        // 客户端应用服务器差异；阶段二补内容
        apply_sync_result(&client, "zjz", &plan.give, &plan.tombstones).unwrap();
        let to_push: Vec<ZNote> = client_all
            .into_iter()
            .filter(|n| plan.need_push.contains(&n.id))
            .collect();
        upsert_notes(&server, "zjz", &to_push).unwrap();

        let s = list_all_notes(&server, "zjz").unwrap();
        let c = list_all_notes(&client, "zjz").unwrap();
        assert_eq!(s, c);
        assert_eq!(s.len(), 3);
        assert_eq!(s.iter().find(|n| n.id == 1).unwrap().content, "client-new");
    }

    #[test]
    fn test_tombstone_propagation_and_revive() {
        let server = init_db(":memory:").unwrap();
        let client = init_db(":memory:").unwrap();
        upsert_notes(&server, "zjz", &[note(1, "a", 10, false)]).unwrap();
        upsert_notes(&client, "zjz", &[note(1, "a", 20, true)]).unwrap();

        // 客户端较新删除赢：墓碑零内容传播，服务器阶段一立即落库
        let plan = sync_phase1(&server, "zjz", &metas_of(&client, "zjz")).unwrap();
        assert!(plan.need_push.is_empty());
        assert!(plan.give.is_empty());
        assert_eq!(plan.tombstones.len(), 1);
        assert!(list_notes_by_name(&server, "zjz").unwrap().is_empty());
        apply_sync_result(&client, "zjz", &plan.give, &plan.tombstones).unwrap();
        assert_eq!(
            list_all_notes(&server, "zjz").unwrap(),
            list_all_notes(&client, "zjz").unwrap()
        );

        // 客户端复活后再同步：服务器要求补内容
        update_note(&client, 1, None, "b").unwrap();
        let plan = sync_phase1(&server, "zjz", &metas_of(&client, "zjz")).unwrap();
        assert_eq!(plan.need_push, vec![1]);
        let to_push: Vec<ZNote> = list_all_notes(&client, "zjz")
            .unwrap()
            .into_iter()
            .filter(|n| plan.need_push.contains(&n.id))
            .collect();
        upsert_notes(&server, "zjz", &to_push).unwrap();
        assert_eq!(list_notes_by_name(&server, "zjz").unwrap().len(), 1);
    }
}