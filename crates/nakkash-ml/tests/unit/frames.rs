use super::*;

fn write(path: &Path, width: u32, height: u32, color: png::ColorType, data: &[u8]) {
    let mut encoder = png::Encoder::new(File::create(path).unwrap(), width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(data)
        .unwrap();
}

#[test]
fn reads_a_folder_of_frames_in_name_order() {
    let root = std::env::temp_dir().join(format!("nakkash-frames-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    write(
        &root.join("00001.png"),
        2,
        1,
        png::ColorType::Rgba,
        &[9, 9, 9, 255, 8, 8, 8, 0],
    );
    write(
        &root.join("00000.png"),
        2,
        1,
        png::ColorType::Rgb,
        &[1, 2, 3, 4, 5, 6],
    );
    fs::write(root.join("notes.txt"), b"skipped").unwrap();
    let mut frames = PngFolder::open(&root).unwrap();
    assert_eq!((frames.len(), frames.size()), (2, (2, 1)));
    assert_eq!(frames.frame(0).unwrap().data, [1, 2, 3, 4, 5, 6]);
    assert_eq!(frames.frame(1).unwrap().data, [9, 9, 9, 8, 8, 8]);
    write(
        &root.join("00002.png"),
        1,
        1,
        png::ColorType::Rgb,
        &[0, 0, 0],
    );
    let mut frames = PngFolder::open(&root).unwrap();
    assert_eq!(
        frames.frame(2).unwrap_err().kind,
        Kind::Invalid,
        "another size"
    );
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(PngFolder::open(&root).unwrap_err().kind, Kind::Io);
}
