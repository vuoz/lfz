use anyhow::Result;

use crate::cli::clean::remove_dir_all;
use crate::output;
use crate::paths;

pub fn run() -> Result<()> {
    let cache_dir = paths::cache_dir()?;

    if cache_dir.exists() {
        let spinner = output::spinner(&format!("Removing all caches: {}", cache_dir.display()));
        remove_dir_all(&cache_dir)?;
        spinner.finish_with_message("All caches cleared.");
    } else {
        output::info("No caches found.");
    }

    Ok(())
}
