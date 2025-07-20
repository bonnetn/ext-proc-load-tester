use std::{io::Write as _, path::Path, time::Duration};

use tokio::{fs::File, io::AsyncWriteExt as _};
use zstd::Encoder;

pub(crate) async fn write(
    directory_path: &Path,
    target_throughput: u64,
    durations: &[Duration],
) -> Result<(), std::io::Error> {
    let file_name = format!("durations_{target_throughput}.json.zst");
    let file_path = directory_path.join(file_name);

    let mut encoder = Encoder::new(Vec::new(), 0)?;

    encoder.write_all(b"[")?;

    let mut has_previous_value = false;
    for duration in durations {
        if has_previous_value {
            encoder.write_all(b",")?;
        }
        encoder.write_all(format!("{}", duration.as_nanos()).as_bytes())?;
        has_previous_value = true;
    }

    encoder.write_all(b"]")?;
    let v = encoder.finish()?;

    let mut f = File::create(file_path).await?;
    f.write_all(&v).await?;

    Ok(())
}
