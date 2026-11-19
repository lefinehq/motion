use super::*;

#[test]
fn csv_fields_keep_quoted_commas_and_quotes() {
    assert_eq!(
        csv_fields(r#"a,"b, c","say ""hi""""#),
        vec!["a".to_owned(), "b, c".to_owned(), r#"say "hi""#.to_owned()]
    );
}

#[test]
fn rows_read_from_csv_name_their_columns() {
    let dir = std::env::temp_dir().join(format!("nakkash-vary-rows-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rows.csv");
    std::fs::write(&path, "$headline,@cta\nSale,Buy now\n\"Fin, de\",Achetez\n").unwrap();
    let rows = read_rows(&path).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["$headline"], "Fin, de");
    assert_eq!(rows[0]["@cta"], "Buy now");
}

#[test]
fn content_leaves_a_slot_that_changes_nothing_drawn() {
    let master: Session = r##"{"nakkash":1,"size":"200x200","dur":"1s",
        "slots":{"old":{"text":"Hi","seen":false},"title":{"text":"Hi"}},
        "layers":[
          {"id":"a","text":"$old","font":"Inter Variable 24 400","fill":"#FFFFFF"},
          {"id":"b","text":"$title","font":"Inter Variable 24 400","fill":"#FFFFFF"}]}"##
        .parse()
        .expect("a valid scene");
    let held = Held::of(master.scene(), master.document().raw());
    let mut raw = master.document().raw().clone();
    let values: BTreeMap<String, String> = [("$old", "Sale"), ("$title", "Sale")]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let note = apply(&mut raw, &master, &held, &values, Path::new(".")).unwrap();
    assert_eq!(raw["slots"]["old"]["text"], serde_json::json!("Hi"));
    assert_eq!(raw["slots"]["title"]["text"], serde_json::json!("Sale"));
    assert!(note.contains("$old: held"), "{note}");
}
