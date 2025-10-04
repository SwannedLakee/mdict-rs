use rusqlite::{named_params, Connection};
use tracing::info;

use crate::config::MDX_FILES;

pub fn query(word: String) -> String {
    let w = word;
    for file in MDX_FILES {
        let db_file = format!("{file}.db");
        let conn = Connection::open(&db_file).unwrap();
        let mut stmt = conn
            .prepare("select * from MDX_INDEX WHERE text= :word limit 1;")
            .unwrap();
        info!("query params={}, dict={}", &w, file);

        let mut rows = stmt.query(named_params! { ":word": w }).unwrap();
        let row = rows.next().unwrap();
        if let Some(row) = row {
            return row.get::<usize, String>(1).unwrap();
        }
    }
    "not found".to_string()
}
