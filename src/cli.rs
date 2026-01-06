use std::sync::Arc;

use color_eyre::eyre::{Report, Result};
use tokio::signal;
use tokio::sync::RwLock;

mod pipeline;
use pipeline::{BusCommandType, PipelineWrapper};

#[tokio::main]
async fn main() -> Result<(), Report> {
    gst::init()?;

    let pipeline = Arc::new(RwLock::new(PipelineWrapper::new()?));
    pipeline.write().await.play()?;
    let pipeline_copy = pipeline.clone();

    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("saw ctrl-c");
            pipeline_copy.write().await.stop()?;
        }
        _ = tokio::spawn(
                async move {
                    loop {
                        let mut pipeline_terminated = false;
                        while let Some(cmd) = pipeline.write().await.handle_pipeline_message() {
                            if cmd == BusCommandType::Eos {
                                pipeline_terminated = true;
                                break;
                            }
                        }

                        if pipeline_terminated {
                            break;
                        }
                    }
                }
            ) => {}
    };

    Ok(())
}
