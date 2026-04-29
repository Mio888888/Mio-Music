use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cover_img_url: String,
    pub source: String,
    pub meta: String,
    pub create_time: String,
    pub update_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistSongRow {
    pub playlist_id: String,
    pub songmid: String,
    pub position: i64,
    pub data: String,
    pub name: String,
    pub singer: String,
    pub album_name: String,
    pub img: String,
}

pub fn init_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS playlists (
            id           TEXT PRIMARY KEY,
            name         TEXT NOT NULL,
            description  TEXT DEFAULT '',
            coverImgUrl  TEXT DEFAULT 'default-cover',
            source       TEXT NOT NULL,
            meta         TEXT DEFAULT '{}',
            createTime   TEXT NOT NULL,
            updateTime   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS playlist_songs (
            playlist_id  TEXT NOT NULL,
            songmid      TEXT NOT NULL,
            position     INTEGER NOT NULL,
            data         TEXT NOT NULL,
            name         TEXT DEFAULT '',
            singer       TEXT DEFAULT '',
            albumName    TEXT DEFAULT '',
            img          TEXT DEFAULT '',
            PRIMARY KEY (playlist_id, songmid),
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_playlist_songs_position ON playlist_songs(playlist_id, position);
        CREATE INDEX IF NOT EXISTS idx_playlist_songs_name ON playlist_songs(playlist_id, name);",
    )?;
    Ok(())
}

// --- Playlist CRUD ---

pub fn list_playlists(conn: &Connection) -> Result<Vec<PlaylistRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, coverImgUrl, source, meta, createTime, updateTime FROM playlists ORDER BY createTime"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PlaylistRow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            cover_img_url: row.get(3)?,
            source: row.get(4)?,
            meta: row.get(5)?,
            create_time: row.get(6)?,
            update_time: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn get_playlist(conn: &Connection, id: &str) -> Result<Option<PlaylistRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, coverImgUrl, source, meta, createTime, updateTime FROM playlists WHERE id = ?1"
    )?;
    let mut rows = stmt.query_map([id], |row| {
        Ok(PlaylistRow {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            cover_img_url: row.get(3)?,
            source: row.get(4)?,
            meta: row.get(5)?,
            create_time: row.get(6)?,
            update_time: row.get(7)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn insert_playlist(conn: &Connection, p: &PlaylistRow) -> Result<()> {
    conn.execute(
        "INSERT INTO playlists (id, name, description, coverImgUrl, source, meta, createTime, updateTime)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![p.id, p.name, p.description, p.cover_img_url, p.source, p.meta, p.create_time, p.update_time],
    )?;
    Ok(())
}

pub fn delete_playlist(conn: &Connection, id: &str) -> Result<usize> {
    // Songs are cascade-deleted via FK
    conn.execute("DELETE FROM playlists WHERE id = ?1", [id])
}

pub fn update_playlist(conn: &Connection, id: &str, name: &str, description: &str) -> Result<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE playlists SET name = ?1, description = ?2, updateTime = ?3 WHERE id = ?4",
        params![name, description, now, id],
    )
}

pub fn update_cover(conn: &Connection, id: &str, cover_url: &str) -> Result<usize> {
    conn.execute(
        "UPDATE playlists SET coverImgUrl = ?1 WHERE id = ?2",
        params![cover_url, id],
    )
}

pub fn playlist_exists(conn: &Connection, id: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM playlists WHERE id = ?1")?;
    let mut rows = stmt.query_map([id], |_| Ok(true))?;
    match rows.next() {
        Some(Ok(true)) => Ok(true),
        _ => Ok(false),
    }
}

// --- Playlist Songs CRUD ---

pub fn list_songs(conn: &Connection, playlist_id: &str) -> Result<Vec<PlaylistSongRow>> {
    let mut stmt = conn.prepare(
        "SELECT playlist_id, songmid, position, data, name, singer, albumName, img
         FROM playlist_songs WHERE playlist_id = ?1 ORDER BY position"
    )?;
    let rows = stmt.query_map([playlist_id], |row| {
        Ok(PlaylistSongRow {
            playlist_id: row.get(0)?,
            songmid: row.get(1)?,
            position: row.get(2)?,
            data: row.get(3)?,
            name: row.get(4)?,
            singer: row.get(5)?,
            album_name: row.get(6)?,
            img: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn count_songs(conn: &Connection, playlist_id: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM playlist_songs WHERE playlist_id = ?1",
        [playlist_id],
        |row| row.get(0),
    )
}

pub fn has_song(conn: &Connection, playlist_id: &str, songmid: &str) -> Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT 1 FROM playlist_songs WHERE playlist_id = ?1 AND songmid = ?2"
    )?;
    let mut rows = stmt.query_map(params![playlist_id, songmid], |_| Ok(true))?;
    match rows.next() {
        Some(Ok(true)) => Ok(true),
        _ => Ok(false),
    }
}

pub fn add_songs(conn: &Connection, playlist_id: &str, songs: &[PlaylistSongRow]) -> Result<()> {
    let max_pos: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlist_songs WHERE playlist_id = ?1",
        [playlist_id],
        |row| row.get(0),
    )?;
    let tx = conn.unchecked_transaction()?;
    for (i, s) in songs.iter().enumerate() {
        let pos = max_pos + 1 + i as i64;
        tx.execute(
            "INSERT OR IGNORE INTO playlist_songs (playlist_id, songmid, position, data, name, singer, albumName, img)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![playlist_id, s.songmid, pos, s.data, s.name, s.singer, s.album_name, s.img],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn add_songs_head(conn: &Connection, playlist_id: &str, songs: &[PlaylistSongRow]) -> Result<()> {
    // Shift existing songs up
    let _count = count_songs(conn, playlist_id)?;
    conn.execute(
        "UPDATE playlist_songs SET position = position + ?1 WHERE playlist_id = ?2",
        params![songs.len() as i64, playlist_id],
    )?;
    let tx = conn.unchecked_transaction()?;
    for (i, s) in songs.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO playlist_songs (playlist_id, songmid, position, data, name, singer, albumName, img)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![playlist_id, s.songmid, i as i64, s.data, s.name, s.singer, s.album_name, s.img],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn remove_song(conn: &Connection, playlist_id: &str, songmid: &str) -> Result<usize> {
    conn.execute(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1 AND songmid = ?2",
        params![playlist_id, songmid],
    )
}

pub fn remove_songs(conn: &Connection, playlist_id: &str, songmids: &[String]) -> Result<usize> {
    if songmids.is_empty() { return Ok(0); }
    let placeholders: Vec<String> = songmids.iter().enumerate().map(|(i, _)| format!("?{}", i + 2)).collect();
    let sql = format!(
        "DELETE FROM playlist_songs WHERE playlist_id = ?1 AND songmid IN ({})",
        placeholders.join(",")
    );
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(playlist_id.to_string())];
    for id in songmids {
        params_vec.push(Box::new(id.clone()));
    }
    let params: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params.as_slice())
}

pub fn clear_songs(conn: &Connection, playlist_id: &str) -> Result<usize> {
    conn.execute("DELETE FROM playlist_songs WHERE playlist_id = ?1", [playlist_id])
}

pub fn search_songs(conn: &Connection, playlist_id: &str, keyword: &str) -> Result<Vec<PlaylistSongRow>> {
    let pattern = format!("%{}%", keyword);
    let mut stmt = conn.prepare(
        "SELECT playlist_id, songmid, position, data, name, singer, albumName, img
         FROM playlist_songs WHERE playlist_id = ?1 AND (name LIKE ?2 OR singer LIKE ?2)
         ORDER BY position"
    )?;
    let rows = stmt.query_map(params![playlist_id, pattern], |row| {
        Ok(PlaylistSongRow {
            playlist_id: row.get(0)?,
            songmid: row.get(1)?,
            position: row.get(2)?,
            data: row.get(3)?,
            name: row.get(4)?,
            singer: row.get(5)?,
            album_name: row.get(6)?,
            img: row.get(7)?,
        })
    })?;
    rows.collect()
}

// --- Favorites ID persistence (stored in a key-value table) ---

pub fn ensure_kv_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value TEXT NOT NULL);"
    )
}

pub fn kv_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM kv_store WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).ok().map_or(Ok(None), |v| Ok(Some(v)))
}

pub fn kv_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO kv_store (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}
