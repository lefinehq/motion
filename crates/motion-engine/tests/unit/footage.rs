//! Unit tests for `src/footage.rs`.

use super::*;

/// A fresh, resolved scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "motion-engine-footage-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create a scratch folder");
    fs::canonicalize(dir).expect("resolve the scratch folder")
}

/// Writes an empty file at `root/relative`, with its folders.
fn touch(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"").unwrap();
    path
}

#[test]
fn a_file_of_the_same_name_is_found_under_the_project_folder() {
    let root = scratch("by-name");
    let photo = touch(&root, "(Footage)/Photos/Beach.JPG");
    let index = Index::of(&root);
    let stored = r"D:\Author\Template\(Footage)\Photos\beach.jpg";
    assert_eq!(locate(stored, &root, &index), Some(photo));
    assert_eq!(locate("/author/elsewhere/missing.jpg", &root, &index), None);
}

#[test]
fn the_file_sharing_most_of_the_stored_folders_wins_then_the_shallowest() {
    let root = scratch("best");
    touch(&root, "a.png");
    touch(&root, "old/renders/a.png");
    let wanted = touch(&root, "deep/Footage/Logos/a.png");
    let index = Index::of(&root);
    let stored = "/Users/author/Project/Footage/Logos/a.png";
    assert_eq!(locate(stored, &root, &index), Some(wanted));
    // Sharing no folder, the shallowest of the same name is taken.
    assert_eq!(
        locate("/elsewhere/a.png", &root, &index),
        Some(root.join("a.png"))
    );
}

#[test]
fn a_stored_path_that_exists_is_taken_as_it_is() {
    let root = scratch("as-stored");
    let elsewhere = touch(&scratch("as-stored-elsewhere"), "logo.png");
    touch(&root, "logo.png");
    let index = Index::of(&root);
    assert_eq!(
        locate(elsewhere.to_str().unwrap(), &root, &index),
        Some(elsewhere)
    );
}

#[test]
fn the_walk_stops_at_its_depth() {
    let root = scratch("depth");
    let deep: String = (0..=DEPTH).map(|i| format!("d{i}/")).collect();
    touch(&root, &format!("{deep}far.png"));
    touch(&root, "near.png");
    let index = Index::of(&root);
    assert!(index.files.contains_key("near.png"));
    assert!(!index.files.contains_key("far.png"));
}

#[test]
fn a_found_file_is_written_relative_to_the_scene_or_absolute() {
    let root = scratch("written");
    let file = touch(&root, "project/(Footage)/a b.png");
    let scene = root.join("out/scenes");
    assert_eq!(
        written(&file, Some(&scene)).as_deref(),
        Some("../../project/(Footage)/a b.png")
    );
    assert_eq!(
        written(&file, Some(&root.join("project"))).as_deref(),
        Some("(Footage)/a b.png")
    );
    assert_eq!(written(&file, None).as_deref(), file.to_str());
}
