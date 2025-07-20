use std::{path::Path, time::Duration};

use tokio::{
    fs::File,
    io::{AsyncWriteExt as _, BufWriter},
};

pub(crate) async fn write(
    directory_path: &Path,
    target_throughput: u64,
    durations: &[Duration],
) -> Result<(), std::io::Error> {
    let file_name = format!("durations_{target_throughput}.json");
    let file_path = directory_path.join(file_name);

    let f = File::create(file_path).await?;
    let mut writer = BufWriter::new(f);

    writer.write_all(b"[").await?;

    let mut has_previous_value = false;
    for duration in durations {
        if has_previous_value {
            writer.write_all(b",").await?;
        }
        writer
            .write_all(format!("{}", duration.as_nanos()).as_bytes())
            .await?;
        has_previous_value = true;
    }

    writer.write_all(b"]").await?;

    writer.flush().await?;

    Ok(())
}
