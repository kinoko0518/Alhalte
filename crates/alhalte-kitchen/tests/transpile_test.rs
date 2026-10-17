//! Tests for HTML5 and EPUB 3.0 transpilation and epubcheck verification.

mod common;

use std::fs;
use std::process::Command;

use common::create_test_room_data;

use alhalte_kitchen::{CliArgs, OutputFormat, RoomArchive, RoomGraph, export_room_to_zip, run};

#[test]
// spec: alhalte-kitchen/transpile HTML5 standalone document
fn test_html5_transpilation_validity() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    let session = graph
        .serialize_by_character_path(1, "prologue", "castle")
        .unwrap();

    let html = session.to_html5();

    // Check HTML5 structure
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<meta charset=\"utf-8\""));
    assert!(html.contains("<title>Journey of Alice: prologue to castle</title>"));

    // Check CSS embedding from saignant
    assert!(html.contains("saignant-log-entry"));
    assert!(html.contains("saignant-log-speaker"));

    // Check scene headings
    assert!(html.contains("<h2 class=\"kitchen-scene-title\">prologue</h2>"));
    assert!(html.contains("<h2 class=\"kitchen-scene-title\">forest</h2>"));
    assert!(html.contains("<h2 class=\"kitchen-scene-title\">castle</h2>"));

    // Check rendered content and speakers
    assert!(html.contains("Alice"));
    assert!(html.contains("Bob"));
    assert!(html.contains("Let us venture into the forest!"));
    assert!(html.contains("The trees are so tall!"));
    assert!(html.contains("We have reached the castle gates."));
}

#[test]
// spec: alhalte-kitchen/transpile EPUB 3.0 epubcheck verification
fn test_epub_transpilation_epubcheck() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();
    let archive = RoomArchive::load_from_slice(&zip_bytes, None).unwrap();
    let graph = RoomGraph::build(&archive).unwrap();

    let session = graph
        .serialize_by_character_path(1, "prologue", "castle")
        .unwrap();

    let epub_bytes = session.to_epub().expect("EPUB transpilation failed");

    // Write to a temporary file and validate with epubcheck
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let epub_file = temp_dir.path().join("session.epub");
    fs::write(&epub_file, epub_bytes).expect("Failed to write epub file");

    let output = Command::new("epubcheck")
        .arg(&epub_file)
        .output()
        .expect("Failed to execute epubcheck");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("epubcheck stdout: {stdout}");
    println!("epubcheck stderr: {stderr}");

    assert!(
        output.status.success(),
        "epubcheck returned non-zero exit code: {stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("0 件のエラー") || stdout.contains("0 errors"),
        "epubcheck detected errors: {stdout}"
    );
}

#[test]
// spec: alhalte-kitchen/cli execution
fn test_cli_execution() {
    let (room, chunks) = create_test_room_data();
    let zip_bytes = export_room_to_zip(&room, &chunks, None).unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let archive_path = temp_dir.path().join("room.zip");
    fs::write(&archive_path, zip_bytes).unwrap();

    // 1. Run CLI for HTML5 output
    let html_out_path = temp_dir.path().join("output.html");
    let cli_args_html = CliArgs {
        archive: archive_path.clone(),
        format: OutputFormat::Html5,
        output: Some(html_out_path.clone()),
        character: Some(1),
        from: Some("prologue".to_string()),
        to: Some("castle".to_string()),
        scenes: Vec::new(),
        room_key: None,
        title: Some("CLI Test Document".to_string()),
    };
    run(cli_args_html).expect("CLI HTML5 run should succeed");
    assert!(html_out_path.exists());
    let html_content = fs::read_to_string(&html_out_path).unwrap();
    assert!(html_content.contains("CLI Test Document"));

    // 2. Run CLI for EPUB output
    let epub_out_path = temp_dir.path().join("output.epub");
    let cli_args_epub = CliArgs {
        archive: archive_path,
        format: OutputFormat::Epub,
        output: Some(epub_out_path.clone()),
        character: None,
        from: None,
        to: None,
        scenes: vec!["prologue".to_string(), "forest".to_string()],
        room_key: None,
        title: Some("CLI EPUB Test".to_string()),
    };
    run(cli_args_epub).expect("CLI EPUB run should succeed");
    assert!(epub_out_path.exists());

    // Validate CLI generated EPUB with epubcheck
    let output = Command::new("epubcheck")
        .arg(&epub_out_path)
        .output()
        .expect("Failed to execute epubcheck");
    assert!(
        output.status.success(),
        "CLI generated EPUB failed epubcheck validation"
    );
}
