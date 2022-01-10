use rusqlite::Connection;
use crate::search::DATABASE_LOCATION;

pub fn set_endpoint(alias: &String, endpoint: &String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
                "UPDATE config
                SET endpoint = :endpoint
                WHERE alias = :alias AND server = :server"
            )
        .unwrap();
    statement
        .execute(&[(":alias", &alias), (":endpoint", &endpoint), (":server", &server)])
        .unwrap();

    statement = connection
        .prepare(
                "INSERT OR IGNORE INTO config
                (server, alias, endpoint) VALUES (:server, :alias, :endpoint)",
        )
        .unwrap();
    statement
        .execute(&[(":server", &server), (":alias", &alias), (":endpoint", &endpoint)])
        .unwrap();
}

pub fn delete_endpoint(alias: &String, server: &String) {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare(
                "DELETE FROM config
                WHERE alias = :alias AND server = :server",
        )
        .unwrap();
    statement.execute(&[(":alias", &alias), (":server", &server)]).unwrap();
}

pub fn all_endpoints(server: &String) -> String {
    let connection = Connection::open(DATABASE_LOCATION).unwrap();
    let mut statement = connection
        .prepare("SELECT alias, endpoint FROM config WHERE server = :server")
        .unwrap();
    let rows = statement.query_map(&[(":server", server)], |row| {
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
