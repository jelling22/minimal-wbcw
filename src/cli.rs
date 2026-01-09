use std::{sync::Arc, time::Duration};

use color_eyre::eyre::{Report, Result};
use tokio::{signal, task::JoinHandle};
use tokio::sync::RwLock;

mod pipeline;
use pipeline::{BusCommandType, PipelineWrapper};

#[tokio::main]
async fn main() -> Result<(), Report> {
    gst::init()?;

    let pipeline = Arc::new(RwLock::new(PipelineWrapper::new()?));
    pipeline.write().await.play()?;
    let pipeline_copy = pipeline.clone();

    let mut watch_eos: JoinHandle<Result<(), Report>> = tokio::spawn(
        async move {
            loop {
                let msg = pipeline.write().await.handle_pipeline_message();
                if let Some(cmd) = msg {
                    if cmd == BusCommandType::Eos {
                        println!("State changed to NULL. Pipeline finsished.");
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(())
        }
    );

    tokio::select! {
        result = &mut watch_eos => {
            match result {
                Ok(_) => println!("Pipeline finished naturally."),
                Err(e) => println!("Bus panicked: {:?}", e)
            }
        }
        _ = signal::ctrl_c() => {
            println!("saw ctrl-c");
            pipeline_copy.write().await.stop()?;
            let _ = watch_eos.await;
            println!("Pipeline shut down manually.");
        }
    };

    Ok(())
}
