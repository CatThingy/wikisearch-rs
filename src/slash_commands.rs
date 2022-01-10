use crate::search::DATABASE_LOCATION;
use rusqlite::Connection;

pub fn set_endpoint(
    alias: &String,
    endpoint: &String,
    server: &String,
) -> Result<String, Box<dyn std::error::Error>> {
    let connection = Connection::open(DATABASE_LOCATION)?;
    let mut statement = connection.prepare(
        "UPDATE config
                SET endpoint = :endpoint
                WHERE alias = :alias AND server = :server",
    )?;
    statement.execute(&[
        (":alias", &alias),
        (":endpoint", &endpoint),
        (":server", &server),
    ])?;

    statement = connection.prepare(
        "INSERT OR IGNORE INTO config
                (server, alias, endpoint) VALUES (:server, :alias, :endpoint)",
    )?;
    statement.execute(&[
        (":server", &server),
        (":alias", &alias),
        (":endpoint", &endpoint),
    ])?;

    Ok(format!("Set alias {} as endpoint {}", alias, endpoint))
}

pub fn delete_endpoint(
    alias: &String,
    server: &String,
) -> Result<String, Box<dyn std::error::Error>> {
    if alias != "default" {
        let connection = Connection::open(DATABASE_LOCATION)?;
        let mut statement = connection.prepare(
            "DELETE FROM config
            WHERE alias = :alias AND server = :server",
        )?;
        statement.execute(&[(":alias", &alias), (":server", &server)])?;
        return Ok(format!("Deleted alias {}", &alias));
    } else {
        return Ok(format!("Can't delete the default endpoint"));
    }
}

pub fn all_endpoints(server: &String) -> Result<String, Box<dyn std::error::Error>> {
    let connection = Connection::open(DATABASE_LOCATION)?;
    let mut statement =
        connection.prepare("SELECT alias, endpoint FROM config WHERE server = :server")?;
    let rows = statement.query_map(&[(":server", server)], |row| {
        Ok(format!(
            "{} | {}",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?
        ))
    })?;

    let mut results = vec!["```".to_string()];

    for row in rows {
        match row {
            Ok(v) => results.push(v),
            Err(e) => println!("{:?}", e),
        }
    }

    results.push("```".to_string());
    Ok(results.join("\n"))
}
