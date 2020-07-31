
use std::collections::HashSet;
use serde::{ Serialize, Deserialize };
use anyhow::Context as _;
use broadcaster::BroadcastChannel;
use async_std::sync::Arc;


#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveChannel {
    id: isize,
    yt_channel_id: Option<String>,
    bb_space_id: Option<String>,
    name: String,
    description: Option<String>,
    photo: Option<String>,
    published_at: String,
    twitter_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Live {
    id: isize,
    yt_video_key: Option<String>,
    bb_video_id: Option<String>,
    
    title: String,
    
    thumbnail: Option<String>,
    
    live_schedule: Option<String>,
    live_start: Option<String>,
    live_end: Option<String>,
    live_viewers: Option<String>,
    
    channel: LiveChannel,
}

#[derive(Debug, Deserialize)]
struct JetriLive {
    live: Vec<Live>,
}

#[derive(Debug, Clone)]
pub struct LiveEvent {
    pub event_type: String,
    pub json: String,
}

pub async fn auto_live_task(broadcast: BroadcastChannel<Arc<LiveEvent>>, active: &mut HashSet<isize>, mock: bool) -> anyhow::Result<()> {
    
    if mock {
        async_std::task::sleep(::std::time::Duration::new(5, 0)).await;
    }
    
    loop {
        
        let body: String = if mock {
            active.clear();
            let body = async_std::fs::read_to_string("./mock_data.json").await?;
            body
        } else {
            let mut res = surf::get("https://api.holotools.app/v1/live").await
                .map_err(|err| anyhow::anyhow!(err))?;
            let body: String = res.body_string().await
                .map_err(|err| anyhow::anyhow!(err))?;
            body
        };
        
        let body_str: &str = &body;
        
        let data: JetriLive = serde_json::from_str(body_str)
            .map_err(|err| {
                log::error!("{}", err);
                
                if let Some(line) = body_str.lines().skip(err.line()-1).next() {
                    let col = err.column();
                    let start = (col - 5).max(0);
                    let end = (col + 500).min(line.len());
                    
                    let sub_str = &line[start..end];
                    let arrow = "     ^";
                    log::error!("{}", sub_str);
                    log::error!("{}", arrow);
                } else {
                    log::error!("Invalid line number.");
                }
                
                err
            })
            .context("parse json")?;
        
        for live in data.live.iter() {
            if !active.contains(&live.id) {
                let json = serde_json::to_string(live)?;
                
                let event = LiveEvent {
                    event_type: "live".into(),
                    json,
                };
                broadcast.send(&Arc::new(event)).await?;
            }
        }
        for live_id in active.iter() {
            if !data.live.iter().any(|x| x.id == *live_id) {
                let event = LiveEvent {
                    event_type: "unlive".into(),
                    json: live_id.to_string(),
                };
                broadcast.send(&Arc::new(event)).await?;
            }
        }
        
        *active = data.live.iter().map(|x| x.id).collect();
        
        async_std::task::sleep(::std::time::Duration::new(25, 0)).await;
    }
}
