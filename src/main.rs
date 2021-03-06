extern crate csv;
#[macro_use] extern crate lazy_static;
extern crate prettytable;
extern crate regex;
extern crate rustyline;
extern crate sqlite3;

use std::env;


fn _normalize_col(col: &String) -> String {
    lazy_static! {
        static ref RE: regex::Regex = regex::Regex::new(r"\(.*?\)").unwrap();
    }
    return RE.replace_all(col, "")
        .to_lowercase()
        .trim()
        .replace(" ", "_")
        .replace(".", "_")
        .replace("?", "");
}

fn _create_table(db: &mut sqlite3::DatabaseConnection, table_name: &str, cols: &Vec<String>) {
    let create_columns =
        cols.iter().map(|c| format!("{} varchar", c)).collect::<Vec<String>>().join(", ");
    db.exec(&format!("CREATE TABLE {} ({})", table_name, create_columns)).unwrap();
}

fn _insert_row(db: &mut sqlite3::DatabaseConnection,
               table_name: &str,
               row: Vec<String>,
               cols: &Vec<String>) {
    let placeholders = cols.iter()
        .enumerate()
        .map(|(idx, _)| format!("${}", idx + 1))
        .collect::<Vec<String>>()
        .join(", ");
    let mut stmt = db.prepare(&format!("INSERT INTO {} VALUES ({})", table_name, placeholders))
        .unwrap();

    for (idx, value) in row.iter().enumerate() {
        stmt.bind_text((idx + 1) as sqlite3::ParamIx, value).unwrap();
    }
    assert!(stmt.execute().step().unwrap().is_none());
}

fn _load_table_from_path(db: &mut sqlite3::DatabaseConnection,
                         table_name: &str,
                         path: String)
                         -> Vec<String> {
    let mut num_rows = 0;
    let mut reader = csv::Reader::from_file(path).unwrap();

    let normalized_cols = reader.headers().unwrap().iter().map(_normalize_col).collect();
    _create_table(db, table_name, &normalized_cols);

    for row in reader.decode() {
        _insert_row(db, table_name, row.unwrap(), &normalized_cols);
        num_rows += 1;
    }

    println!(
        "Loaded {} rows into {}({})",
        num_rows,
        table_name,
        normalized_cols.join(", "),
    );
    return normalized_cols;
}

fn _print_table(conn: &mut sqlite3::DatabaseConnection, line: &str) {
    let mut table = prettytable::Table::new();
    table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    let mut stmt = match conn.prepare(&line) {
        Ok(stmt) => stmt,
        Err(e) => {
            println!("{}", e.detail.unwrap());
            return;
        }
    };
    let mut results = stmt.execute();
    while let Some(r) = results.step().unwrap() {
        let mut row = prettytable::row::Row::new(vec![]);
        for i in 0..r.column_count() {
            row.add_cell(prettytable::cell::Cell::new(&r.column_text(i).unwrap_or("".to_string())));
        }
        table.add_row(row);
    }
    table.printstd();
}

struct SimpleWordCompleter {
    words: Vec<String>,
}


static BREAK_CHARS: [char; 4] = [' ', '(', ')', ','];
impl SimpleWordCompleter {
    fn new(words: Vec<String>) -> SimpleWordCompleter {
        return SimpleWordCompleter { words: words };
    }
}

impl rustyline::completion::Completer for SimpleWordCompleter {
    fn complete(&self, line: &str, pos: usize) -> rustyline::Result<(usize, Vec<String>)> {
        let (start, word) =
            rustyline::completion::extract_word(line, pos, &BREAK_CHARS.iter().cloned().collect());

        let matches = self.words.iter().filter(|w| w.starts_with(word)).cloned().collect();
        return Ok((start, matches));
    }
}

fn main() {
    let mut paths = env::args().skip(1);

    let mut conn = sqlite3::DatabaseConnection::in_memory().unwrap();

    let mut base_words = vec!["distinct", "select", "from", "group", "by", "order", "where",
                              "count"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    if paths.len() == 1 {
        let mut col_names = _load_table_from_path(&mut conn, "t", paths.next().unwrap());
        base_words.append(&mut col_names);
    } else {
        for (idx, path) in paths.enumerate() {
            let mut col_names = _load_table_from_path(&mut conn, &format!("t{}", idx + 1), path);
            base_words.append(&mut col_names);
        }
    }

    let completer = SimpleWordCompleter::new(base_words);
    let mut rl = rustyline::Editor::new();
    rl.set_completer(Some(completer));
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }
                rl.add_history_entry(&line);
                _print_table(&mut conn, &line);
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("Interrupted");
                continue;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                println!("Error: {}", err);
                break;
            }
        }
    }
}
