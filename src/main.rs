use std::path::PathBuf;

use clap::{Parser, Subcommand};
use eyre::{Context, Result};

mod fdcemu;
mod kh940;
mod nibble;
mod util;

use fdcemu::{Disk, FdcServer};
use kh940::{MachineState, Pattern};
pub use nibble::Nibble;

#[derive(Subcommand)]
enum Command {
    /// Emulate being a floppy drive on a USB->FTDI port
    Emulate { port: PathBuf, disk: PathBuf },

    /// Extract images from a disk image into a folder
    Export { disk: PathBuf, target: PathBuf },

    /// Import images from a folder into a disk image ready for emulation
    Import { disk: PathBuf, source: PathBuf },
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Emulate { port, disk } => {
            let port =
                serial::open(&port).context(format!("Could not open serial port at {port:?}"))?;
            let mut fdc_server = FdcServer::new(&disk, port)?;

            fdc_server.run()?;
        }
        Command::Export {
            disk: disk_path,
            target,
        } => {
            let mut disk = Disk::new();
            disk.load(&disk_path)
                .context(format!("Could not read disk data from {disk_path:?}"))?;
            let machine_state = MachineState::from_memory_dump(&disk.flatten_data());
            if !target.exists() {
                std::fs::create_dir_all(&target)
                    .context(format!("Could not create target folder at {target:?}"))?;
            }

            for pattern in machine_state.patterns() {
                let image = pattern.to_image();
                image.save(target.join(format!("{}.png", pattern.pattern_number())))?;
            }
        }
        Command::Import {
            disk: disk_path,
            source,
        } => {
            let mut disk = Disk::new();
            disk.load(&disk_path)
                .context(format!("Could not read disk data from {disk_path:?}"))?;
            let mut machine_state = MachineState::from_memory_dump(&disk.flatten_data());

            for entry in source
                .read_dir()
                .context(format!("Could not read source folder at {source:?}"))?
            {
                let entry = entry?;

                let path = entry.path();
                let pattern_number = path
                    .file_stem()
                    .and_then(|f| f.to_str())
                    .and_then(|f| f.parse::<u16>().ok());
                let extension = path.extension().and_then(|f| f.to_str());
                if let (Some(pattern_number), Some("png")) = (pattern_number, extension) {
                    let image =
                        image::open(&path).context(format!("Could not read file at {path:?}"))?;
                    let grayscale = image::imageops::grayscale(&image);

                    let pattern = Pattern::from_image(pattern_number, &grayscale)
                        .context(format!("Could not read file at {path:?}"))?;
                    machine_state.add_pattern(pattern);
                }
            }

            let data = machine_state.serialize();
            disk.set_flattened_data(data)?;
            disk.save(&disk_path)?;
        }
    }

    Ok(())
}
