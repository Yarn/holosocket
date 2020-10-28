
use anyhow::Result;
use tide::Request;
use http_types::headers::HeaderValue;
use tide::security::{CorsMiddleware, Origin};
use async_std::task;
use futures::stream::StreamExt;
use async_std::sync::Arc;
use async_std::sync::RwLock;
use async_std::sync::{ channel, Sender };

mod auto_stream_live;
use auto_stream_live::{ LiveEvent, JetriLive, LiveTaskState };

type SseChannels = Arc<RwLock<Vec<Sender<Arc<LiveEvent>>>>>;
type Current = Arc<RwLock<JetriLive>>;

#[derive(Clone)]
struct State {
    sse_channels: Arc<RwLock<Vec<Sender<Arc<LiveEvent>>>>>,
    current: Current,
}

async fn async_main() -> Result<()> {
    
    let sse_channels = Arc::new(RwLock::new(Vec::new()));
    let current = Arc::new(RwLock::new(JetriLive::default()));
    
    let task_sse_channels = sse_channels.clone();
    let task_current = current.clone();
    task::spawn(async {
        let mock = std::env::vars()
            .find(|(x, _)| x == "holosocket_mock")
            .map(|(_, x)| x != "")
            .unwrap_or(false);
        
        let task_sse_channels = task_sse_channels;
        let task_current = task_current;
        let mut state = LiveTaskState::new(task_current.clone());
        loop {
            match auto_stream_live::auto_live_task(task_sse_channels.clone(), &mut state, mock).await {
                Ok(()) => {}
                Err(err) => { dbg!(err); },
            }
            async_std::task::sleep(::std::time::Duration::new(25, 0)).await;
        }
    });
    
    let cors = CorsMiddleware::new()
        .allow_methods("GET, POST, OPTIONS".parse::<HeaderValue>().unwrap())
        .allow_origin(Origin::from("*"))
        .allow_credentials(false);
    
    let state = State {
        sse_channels,
        current,
    };
    let mut app = tide::with_state(state);
    
    app.with(cors);
    
    app.at("/check").get(|_req| async move {
        Ok("OK")
    });
    app.at("/sse").get(|req| async move {
        let mut res = tide::sse::upgrade(req, |req: Request<State>, sender| async move {
            let (send, mut recv) = channel(100);
            {
                let mut sse_channels = req.state().sse_channels.write().await;
                sse_channels.push(send);
            }
            let json = {
                let current = req.state().current.read().await;
                serde_json::to_string(&*current)?
            };
            sender.send("initial", &json, None).await?;
            
            loop {
                let val = match recv.next().await {
                    Some(x) => x,
                    None => break,
                };
                
                // log::trace!("{:?}", val);
                
                sender.send(&val.event_type, &val.json, None).await?;
            }
            Ok(())
        });
        
        res.insert_header("X-Accel-Buffering", "no");
        
        Ok(res)
    });
    
    let port: usize = std::env::vars()
        .find(|(x, _)| x == "holosocket_port")
        .map(|(_, x)| x)
        .unwrap_or("8080".to_string())
        .parse()?;
    let bind_addr = format!("0.0.0.0:{}", port);
    app.listen(bind_addr.as_str()).await?;
    
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
