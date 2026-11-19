use super::*;

#[test]
fn an_opaque_matte_keeps_a_pixel_and_a_clear_one_clears_it() {
    let mut pixels = vec![
        10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 200, 1, 2, 3, 0,
    ];
    cut_alpha(&mut pixels, &[65535, 0, 32768, 65535]);
    // Colour is untouched: alpha is straight.
    assert_eq!(
        pixels,
        [10, 20, 30, 255, 10, 20, 30, 0, 10, 20, 30, 100, 1, 2, 3, 0]
    );
}

mod baked {
    use nakkash_baked::{Alpha, Made, MatteWriter};
    use nakkash_fonts::Cache;
    use nakkash_format::Content;
    use nakkash_raster::RenderOptions;

    use super::*;
    use crate::Session;

    /// The 32x16 test clip of the video crate, ten frames.
    const CLIP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../nakkash-video/tests/fixtures/rgb.mov"
    );

    /// A scene of the clip over black, its layer with `cutout`, or with
    /// none when it is empty.
    fn scene(cutout: &str) -> String {
        let cutout = match cutout {
            "" => String::new(),
            cutout => format!(r#","cutout":{cutout}"#),
        };
        format!(
            r##"{{"nakkash":1,"size":"32x16","fps":10,"dur":"1s","bg":"#000000","layers":[{{"id":"v","video":"{CLIP}","box":[32,16],"at":"16 8"{cutout}}}]}}"##
        )
    }

    /// A session of `scene` reading its cache, and so its bakes, from `root`.
    fn session(scene: &str, root: &Path) -> Session {
        let mut session: Session = scene.parse().expect("a valid scene");
        session.set_font_cache(Cache::new(root));
        session
    }

    /// A cache folder of its own for test `name`.
    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "nakkash-engine-baked-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    const PROMPT: &str = r#"{"prompts":[{"frame":0,"box":[0,0,16,16]}]"#;

    /// Bakes, into the cache at `root`, a matte of every clip frame that
    /// keeps the left half and clears the right, for the cutout `PROMPT`
    /// asks for; its digest.
    fn bake(root: &Path) -> Sha256 {
        let unbaked = session(&scene(&format!("{PROMPT}}}")), root);
        let Content::Video(video) = &unbaked.scene().root.layers[0].content else {
            panic!("a video layer");
        };
        let request = video.cutout.as_ref().expect("a cutout").request();
        let store = unbaked.baked_store().unwrap();
        let frames = Clip::open(Path::new(CLIP)).unwrap().frames();
        let half: Vec<f32> = (0..32 * 16)
            .map(|i| if i % 32 < 16 { 1.0 } else { 0.0 })
            .collect();
        let mut writer = MatteWriter::new(&store, [32, 16]);
        for _ in 0..frames {
            writer.push(&Alpha::from_unit(32, 16, &half)).unwrap();
        }
        let (clip, _) = Sha256::of_file(Path::new(CLIP)).unwrap();
        writer.finish(request, clip, Made::here(1)).unwrap()
    }

    #[test]
    fn a_baked_cutout_keeps_the_object_and_clears_the_rest() {
        let root = root("cut");
        let digest = bake(&root);
        let cut = session(&scene(&format!(r#"{PROMPT},"baked":"{digest}"}}"#)), &root);
        let plain = session(&scene(""), &root);
        for frame in [0.0, 4.0] {
            let drawn = cut.render_with(frame, RenderOptions::GOLDEN).unwrap();
            let whole = plain.render_with(frame, RenderOptions::GOLDEN).unwrap();
            for (i, (a, b)) in drawn
                .pixels()
                .chunks_exact(4)
                .zip(whole.pixels().chunks_exact(4))
                .enumerate()
            {
                match i % 32 < 16 {
                    true => assert_eq!(
                        a, b,
                        "pixel {i} of frame {frame}: kept as the clip draws it"
                    ),
                    false => assert_eq!(
                        &a[..3],
                        [0, 0, 0],
                        "pixel {i} of frame {frame}: cleared to the background"
                    ),
                }
            }
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_unbaked_cutout_names_the_bake() {
        let root = root("unbaked");
        let error = session(&scene(&format!("{PROMPT}}}")), &root)
            .render(0.0)
            .unwrap_err();
        assert_eq!(error.kind, Kind::Missing);
        assert!(error.message.contains(BAKE_COMMAND), "{}", error.message);
        assert!(error.path.unwrap().to_string().ends_with(CUTOUT));
    }

    #[test]
    fn a_bake_for_another_prompt_is_refused() {
        let root = root("stale");
        let digest = bake(&root);
        let moved = r#"{"prompts":[{"frame":0,"box":[1,0,16,16]}]"#;
        let error = session(&scene(&format!(r#"{moved},"baked":"{digest}"}}"#)), &root)
            .render(0.0)
            .unwrap_err();
        assert_eq!(error.kind, Kind::Conflict);
        assert!(error.message.contains(BAKE_COMMAND), "{}", error.message);
        // A digest the cache lacks.
        let absent = Sha256::of(b"never baked");
        let error = session(&scene(&format!(r#"{PROMPT},"baked":"{absent}"}}"#)), &root)
            .render(0.0)
            .unwrap_err();
        assert_eq!(error.kind, Kind::Missing);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
