use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;
use wxupd::resource::dict::DictResource;
use wxupd::resource::Resource;
use wxupd::safe_list::SafeList;

fn build_fake_dicts_zip(out: &std::path::Path) {
    let f = fs::File::create(out).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
    // Bare directory entry, like the real zip
    z.add_directory("dicts/", opts).unwrap();
    z.start_file("dicts/jichu.dict.yaml", opts).unwrap();
    z.write_all(b"name: jichu").unwrap();
    z.start_file("dicts/shici.dict.yaml", opts).unwrap();
    z.write_all(b"name: shici").unwrap();
    z.finish().unwrap();
}

#[tokio::test]
async fn install_extracts_yamls_preserving_dicts_prefix() {
    let d = TempDir::new().unwrap();
    let zip = d.path().join("dicts.zip");
    build_fake_dicts_zip(&zip);
    let rime = d.path().join("rime");
    let safe = SafeList::defaults_plus(&[]).unwrap();
    let report = DictResource.install(&zip, &rime, &safe).await.unwrap();

    assert!(report
        .files_written
        .iter()
        .any(|p| p == Path::new("dicts/jichu.dict.yaml")));
    assert!(report
        .files_written
        .iter()
        .any(|p| p == Path::new("dicts/shici.dict.yaml")));
    assert!(report.files_skipped.is_empty());
    assert_eq!(
        fs::read(rime.join("dicts/jichu.dict.yaml")).unwrap(),
        b"name: jichu"
    );
}
