//! Bounded original-cursor export, with all rows from one read transaction.
use super::{BrowseError, LibraryCursor};
use serde_json::{Value, json};

impl LibraryCursor {
    pub fn export_mobile_backup(
        &mut self,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Value, BrowseError> {
        self.validate()?;
        check().map_err(|_| BrowseError::Unavailable)?;
        let result = match self.connection.as_mut() {
            None => {
                super::backup_restore::check_missing_sidecars(&self.path)?;
                Ok(json!({"playlists": []}))
            }
            Some(connection) => export_on(connection, check),
        };
        self.validate()?;
        check().map_err(|_| BrowseError::Unavailable)?;
        result
    }
}

fn export_on(
    connection: &mut rusqlite::Connection,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<Value, BrowseError> {
    let unavailable = |_| BrowseError::Unavailable;
    let transaction = connection.transaction().map_err(unavailable)?;
    let (playlists, songs): (i64, i64) = transaction
        .query_row(
            "SELECT (SELECT COUNT(*) FROM playlists), (SELECT COUNT(*) FROM playlist_songs)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(unavailable)?;
    if !(0..=1000).contains(&playlists) || !(0..=20000).contains(&songs) {
        return Err(BrowseError::Unavailable);
    }
    let orphan: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM playlist_songs s WHERE NOT EXISTS(SELECT 1 FROM playlists p WHERE p.id=s.playlist_id))", [], |row| row.get(0),
    ).map_err(unavailable)?;
    if orphan {
        return Err(BrowseError::Unavailable);
    }
    let mut used = 0usize;
    for sql in [
        "SELECT COALESCE(SUM(length(CAST(COALESCE(id,'') AS BLOB))+length(CAST(COALESCE(name,'') AS BLOB))+length(CAST(COALESCE(description,'') AS BLOB))+length(CAST(COALESCE(coverImgUrl,'') AS BLOB))+length(CAST(COALESCE(source,'') AS BLOB))+length(CAST(COALESCE(meta,'') AS BLOB))+length(CAST(COALESCE(createTime,'') AS BLOB))+length(CAST(COALESCE(updateTime,'') AS BLOB))),0) FROM playlists",
        "SELECT COALESCE(SUM(length(CAST(COALESCE(playlist_id,'') AS BLOB))+length(CAST(COALESCE(songmid,'') AS BLOB))+length(CAST(COALESCE(data,'') AS BLOB))+length(CAST(COALESCE(name,'') AS BLOB))+length(CAST(COALESCE(singer,'') AS BLOB))+length(CAST(COALESCE(albumName,'') AS BLOB))+length(CAST(COALESCE(img,'') AS BLOB))),0) FROM playlist_songs",
    ] {
        check().map_err(|_| BrowseError::Unavailable)?;
        let bytes: i64 = transaction
            .query_row(sql, [], |row| row.get(0))
            .map_err(unavailable)?;
        used = used
            .checked_add(usize::try_from(bytes).map_err(|_| BrowseError::Unavailable)?)
            .filter(|n| *n <= crate::backup_restore::MAX_DOCUMENT_BYTES)
            .ok_or(BrowseError::Unavailable)?;
    }
    let mut items = Vec::with_capacity(playlists as usize);
    {
        let mut query = transaction.prepare("SELECT id,name,description,coverImgUrl,source,meta,createTime,updateTime FROM playlists ORDER BY createTime,id").map_err(unavailable)?;
        let mut rows = query.query([]).map_err(unavailable)?;
        let mut songs_query = transaction.prepare("SELECT songmid,position,data,name,singer,albumName,img FROM playlist_songs WHERE playlist_id=?1 ORDER BY position,songmid").map_err(unavailable)?;
        while let Some(row) = rows.next().map_err(unavailable)? {
            check().map_err(|_| BrowseError::Unavailable)?;
            let id: String = row.get(0).map_err(unavailable)?;
            let playlist = json!({"id":id,"name":row.get::<_,String>(1).map_err(unavailable)?,
                "description":row.get::<_,String>(2).map_err(unavailable)?,"coverImgUrl":row.get::<_,String>(3).map_err(unavailable)?,
                "source":row.get::<_,String>(4).map_err(unavailable)?,"meta":row.get::<_,String>(5).map_err(unavailable)?,
                "createTime":row.get::<_,String>(6).map_err(unavailable)?,"updateTime":row.get::<_,String>(7).map_err(unavailable)?});
            let mut song_rows = songs_query.query([&id]).map_err(unavailable)?;
            let mut saved = Vec::new();
            while let Some(row) = song_rows.next().map_err(unavailable)? {
                check().map_err(|_| BrowseError::Unavailable)?;
                saved.push(json!({"songmid":row.get::<_,String>(0).map_err(unavailable)?,"position":row.get::<_,i64>(1).map_err(unavailable)?,
                    "data":row.get::<_,String>(2).map_err(unavailable)?,"name":row.get::<_,String>(3).map_err(unavailable)?,
                    "singer":row.get::<_,String>(4).map_err(unavailable)?,"albumName":row.get::<_,String>(5).map_err(unavailable)?,"img":row.get::<_,String>(6).map_err(unavailable)?}));
            }
            items.push(Value::Object(
                [
                    ("playlist".into(), playlist),
                    ("songs".into(), Value::Array(saved)),
                ]
                .into_iter()
                .collect(),
            ));
        }
    }
    transaction.commit().map_err(unavailable)?;
    Ok(Value::Object(
        [("playlists".into(), Value::Array(items))]
            .into_iter()
            .collect(),
    ))
}
