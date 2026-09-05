use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Create a test WAV file with basic content.
fn create_test_wav(path: &Path, sample_count: usize) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;

    // Write a simple ramp
    for i in 0..sample_count {
        let sample = ((i % 1000) as i16 * 32);
        writer.write_sample(sample)?;
    }

    writer.finalize()?;
    Ok(())
}

/// Create a minimal stand set for testing.
fn create_test_stand_set(dir: &TempDir) -> anyhow::Result<()> {
    // Create audio directory
    let audio_dir = dir.path().join("аудио");
    fs::create_dir(&audio_dir)?;

    // Create test WAV files
    create_test_wav(&audio_dir.join("test1.wav"), 16000)?; // 1 second
    create_test_wav(&audio_dir.join("test2.wav"), 32000)?; // 2 seconds

    // Create эталоны.json
    let metadata = serde_json::json!({
        "частота": 16000,
        "случаев": 2,
        "случаи": [
            {
                "файл": "test1.wav",
                "критерий": "F-01",
                "эталон": "hello world",
                "голос": "Test",
                "темп": 120,
                "длительность": 1.0,
                "особенность": "test"
            },
            {
                "файл": "test2.wav",
                "критерий": "AM-02",
                "эталон": "good morning",
                "голос": "Test",
                "темп": 120,
                "длительность": 2.0,
                "особенность": "test"
            }
        ]
    });

    fs::write(
        dir.path().join("эталоны.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    Ok(())
}

#[test]
fn test_integration_mock_backend() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    create_test_stand_set(&dir)?;

    // Run the benchmark with mock backend
    let output = std::process::Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "whspr-bench",
            "--",
            "--stand-set",
            dir.path().to_str().unwrap(),
            "--asr",
            "mock",
        ])
        .output()?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8(output.stdout)?;

    // Check for expected output sections
    assert!(stdout.contains("Per-Case Results"));
    assert!(stdout.contains("Per-Criterion-Group Results"));
    assert!(stdout.contains("Aggregate Results"));
    assert!(stdout.contains("test1.wav"));
    assert!(stdout.contains("test2.wav"));
    assert!(stdout.contains("F-01"));
    assert!(stdout.contains("AM-02"));

    Ok(())
}

#[test]
fn test_integration_json_output() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    create_test_stand_set(&dir)?;

    let output = std::process::Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "whspr-bench",
            "--",
            "--stand-set",
            dir.path().to_str().unwrap(),
            "--asr",
            "mock",
            "--json",
        ])
        .output()?;

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8(output.stdout)?;

    // Parse as JSON to ensure it's valid
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    // Check JSON structure
    assert!(json.get("cases").is_some());
    assert!(json.get("groups").is_some());
    assert!(json.get("aggregate").is_some());

    let cases = json.get("cases").unwrap().as_array().unwrap();
    assert_eq!(cases.len(), 2);

    // Check first case
    assert_eq!(cases[0].get("file").unwrap().as_str().unwrap(), "test1.wav");
    assert_eq!(
        cases[0].get("criterion").unwrap().as_str().unwrap(),
        "F-01"
    );

    Ok(())
}

#[test]
fn test_integration_criterion_grouping() -> anyhow::Result<()> {
    let dir = TempDir::new()?;

    // Create audio directory
    let audio_dir = dir.path().join("аудио");
    fs::create_dir(&audio_dir)?;

    // Create test WAV files
    create_test_wav(&audio_dir.join("a.wav"), 16000)?;
    create_test_wav(&audio_dir.join("b.wav"), 16000)?;
    create_test_wav(&audio_dir.join("c.wav"), 16000)?;

    // Create эталоны.json with multiple criteria
    let metadata = serde_json::json!({
        "частота": 16000,
        "случаев": 3,
        "случаи": [
            {
                "файл": "a.wav",
                "критерий": "F-01",
                "эталон": "test one",
                "голос": "Test",
                "темп": 120,
                "длительность": 1.0,
                "особенность": "test"
            },
            {
                "файл": "b.wav",
                "критерий": "F-02",
                "эталон": "test two",
                "голос": "Test",
                "темп": 120,
                "длительность": 1.0,
                "особенность": "test"
            },
            {
                "файл": "c.wav",
                "критерий": "AM-01",
                "эталон": "test three",
                "голос": "Test",
                "темп": 120,
                "длительность": 1.0,
                "особенность": "test"
            }
        ]
    });

    fs::write(
        dir.path().join("эталоны.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    let output = std::process::Command::new("cargo")
        .args(&[
            "run",
            "-p",
            "whspr-bench",
            "--",
            "--stand-set",
            dir.path().to_str().unwrap(),
            "--asr",
            "mock",
            "--json",
        ])
        .output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    let groups = json.get("groups").unwrap().as_array().unwrap();

    // Should have 2 groups: F (2 cases) and AM (1 case)
    assert_eq!(groups.len(), 2);

    // Find F group
    let f_group = groups.iter().find(|g| g.get("prefix").unwrap().as_str().unwrap() == "F");
    assert!(f_group.is_some());
    assert_eq!(
        f_group.unwrap().get("case_count").unwrap().as_u64().unwrap(),
        2
    );

    // Find AM group
    let am_group = groups
        .iter()
        .find(|g| g.get("prefix").unwrap().as_str().unwrap() == "AM");
    assert!(am_group.is_some());
    assert_eq!(
        am_group.unwrap().get("case_count").unwrap().as_u64().unwrap(),
        1
    );

    Ok(())
}
