
use anyhow::Result;
use tide::Request;
use http_types::headers::HeaderValue;
use tide::security::{CorsMiddleware, Origin};
use async_std::task;
use std::collections::HashSet;
use broadcaster::BroadcastChannel;
use futures::stream::StreamExt;
use async_std::sync::Arc;

mod auto_stream_live;
use auto_stream_live::LiveEvent;

#[derive(Clone)]
struct State {
    broadcast: BroadcastChannel<Arc<LiveEvent>>,
}

async fn async_main() -> Result<()> {
    
    let broadcast: BroadcastChannel<Arc<LiveEvent>> = BroadcastChannel::new();
    
    let task_broadcast = broadcast.clone();
    task::spawn(async {
        let mut active = HashSet::new();
        let mock = std::env::vars()
            .find(|(x, _)| x == "holosocket_mock")
            .map(|(_, x)| x != "")
            .unwrap_or(false);
        
        match auto_stream_live::auto_live_task(task_broadcast, &mut active, mock).await {
            Ok(()) => {}
            Err(err) => { dbg!(err); },
        }
    });
    
    let cors = CorsMiddleware::new()
        .allow_methods("GET, POST, OPTIONS".parse::<HeaderValue>().unwrap())
        .allow_origin(Origin::from("*"))
        .allow_credentials(false);
    
    let state = State {
        broadcast,
    };
    let mut app = tide::with_state(state);
    
    app.middleware(cors);
    
    app.at("/sse").get(|req| async move {
        let mut res = tide_compressed_sse::upgrade(req, |req: Request<State>, sender| async move {
            let mut broadcast = req.state().broadcast.clone();
            
            loop {
                let val = match broadcast.next().await {
                    Some(x) => x,
                    None => break,
                };
                log::trace!("{:?}", val);
                
                sender.send(&val.event_type, &val.json, None).await?;
            }
            Ok(())
        });
        
        res.insert_header("X-Accel-Buffering", "no");
        
        Ok(res)
    });
    app.listen("0.0.0.0:8080").await?;
    
    dbg!();
    Ok(())
}

fn main() -> Result<()> {
    let debug = std::env::vars()
        .find(|(x, _)| x == "holosocket_debug")
        .map(|(_, x)| x != "")
        .unwrap_or(false);
    
    let log_level = if debug {
        log::LevelFilter::Trace
    } else {
        log::LevelFilter::Warn
    };
    
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]"),
                // "[{}][{}] {}",
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Warn)
        .level_for("holosocket", log_level)
        .chain(std::io::stdout())
        .apply()?;
    
    futures::executor::block_on(async_main())?;
    
    Ok(())
}
