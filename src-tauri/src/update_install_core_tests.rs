use super::*;

fn archive(payload: &[u8]) -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gzip);
    let mut header = tar::Header::new_gnu();
    header.set_size(payload.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(
        &mut header,
        "TUICommander.app/Contents/MacOS/tuicommander",
        payload,
    )
    .unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

// Catches: converting tar's error with Display loses the actual filesystem failure.
#[cfg(unix)]
#[test]
fn tar_io_cause_is_retained_when_unpack_destination_is_directory() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let bytes = archive(b"test executable");
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
    let mut entry = tar.entries().unwrap().next().unwrap().unwrap();
    let error = entry.unpack(dir.path()).unwrap_err();
    let message = error_chain(&error);
    assert!(
        message.contains("TUICommander.app/Contents/MacOS/tuicommander"),
        "{message}"
    );
    assert!(message.contains("os error "), "{message}");
    assert!(message.contains("Caused by:"), "{message}");
}

// Catches: a plain error is duplicated or its text altered by cause formatting.
#[test]
fn plain_io_error_remains_readable_without_duplicate_lines() {
    assert_eq!(
        error_chain(&io::Error::other("download failed")),
        "download failed"
    );
}

// Catches: a same-sized compressed download is used instead of extracted sizes,
// or individual files are not rounded to disk allocation units.
#[cfg(unix)]
#[test]
fn archive_space_counts_uncompressed_files_and_metadata() {
    let bytes = archive(&[0; 4097]);
    assert!(bytes.len() < 4097);
    assert_eq!(
        archive_required_space(&bytes, 4096).unwrap(),
        1024 * 1024 + 8192
    );
}

// Catches: equality is rejected, the insufficient-space check is inverted, or
// the diagnostic omits the concrete required/available byte counts.
#[cfg(unix)]
#[test]
fn capacity_refuses_only_insufficient_space_and_names_both_byte_counts() {
    let root = std::path::Path::new("/update-volume");
    assert!(check_capacity(1024, 1024, root).is_ok());
    assert!(check_capacity(1024, 1025, root).is_ok());
    let error = check_capacity(1024, 1023, root).unwrap_err();
    assert!(
        error.contains("requires 1024 bytes, available 1023 bytes"),
        "{error}"
    );
    assert!(error.contains("/update-volume"), "{error}");
}

// Catches: malformed downloads pass the preflight and fail later without context.
#[cfg(unix)]
#[test]
fn invalid_archive_is_rejected_before_install() {
    let error =
        archive_space_check(b"not a gzip", &crate::test_support::test_temp_root()).unwrap_err();
    assert_ne!(error, "");
}

// Catches: real free-space measurement uses the wrong volume, or an oversized
// update begins extraction instead of refusing with required and available bytes.
// Run explicitly on a mounted 60 MiB test image with the recorded release asset.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires TUIC_UPDATE_TEST_VOLUME (60 MiB image) and TUIC_UPDATE_TEST_ASSET"]
fn small_disk_image_refuses_real_update_before_any_extraction() {
    let volume =
        std::path::PathBuf::from(std::env::var_os("TUIC_UPDATE_TEST_VOLUME").expect("volume"));
    let bytes = std::fs::read(std::env::var_os("TUIC_UPDATE_TEST_ASSET").expect("asset")).unwrap();
    let before = std::fs::read_dir(&volume).unwrap().count();
    let error = archive_space_check(&bytes, &volume).unwrap_err();
    assert!(error.contains("Not enough free space"), "{error}");
    assert!(
        error.contains("requires ") && error.contains("bytes, available "),
        "{error}"
    );
    assert_eq!(std::fs::read_dir(volume).unwrap().count(), before);
}

// Catches: the plugin's same-volume fallback is ignored when TMPDIR is elsewhere.
#[cfg(target_os = "macos")]
#[test]
fn extraction_uses_app_parent_when_system_temp_is_on_another_volume() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let app = dir.path().join("Test.app");
    std::fs::create_dir(&app).unwrap();
    let missing = dir.path().join("missing-temp");
    assert_eq!(extraction_root(&app, &missing).unwrap(), dir.path());
    assert_eq!(extraction_root(&app, dir.path()).unwrap(), dir.path());
}
