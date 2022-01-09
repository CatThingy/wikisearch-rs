use rusqlite::Connection;
use crate::search::DATABASE_LOCATION;

pub fn set_endpoint(alias: &String, endpoint: &String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
            format!(
                "UPDATE {}
                SET endpoint = :endpoint
                WHERE alias = :alias",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement
        .execute(&[(":alias", &alias), (":endpoint", &endpoint)])
        .unwrap();

    statement = connection
        .prepare(
            format!(
                "INSERT OR IGNORE INTO {}
                (alias, endpoint) VALUES (:alias, :endpoint)",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement
        .execute(&[(":alias", &alias), (":endpoint", &endpoint)])
        .unwrap();
}

pub fn delete_endpoint(alias: &String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
            format!(
                "DELETE FROM {}
                WHERE alias = :alias",
                server
            )
            .as_str(),
        )
        .unwrap();
    statement.execute(&[(":alias", &alias)]).unwrap();
}

pub fn all_endpoints(server: &String) -> String {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(format!("SELECT alias, endpoint FROM {}", server).as_str())
        .unwrap();
    let rows = statement.query_map([], |row| {
        Ok(format!("{} | {}",row.get::<_, String>(0).unwrap(), row.get::<_, String>(1).unwrap()))
    }).unwrap();

    let mut results = vec!["```".to_string()];

    for row in rows {
        match row {
            Ok(v) => results.push(v),
            Err(e) => println!("{:?}", e),
        }
    }

    results.push("```".to_string());
    results.join("\n")
}
