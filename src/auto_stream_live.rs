
use std::collections::HashSet;
use serde::{ Serialize, Deserialize };
use anyhow::Context as _;
use async_std::sync::Arc;
use crate::SseChannels;

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
    live_viewers: Option<isize>,
    
    channel: LiveChannel,
}

#[derive(Debug, Deserialize)]
struct JetriLive {
    live: Vec<Live>,
    upcoming: Vec<Live>,
}

#[derive(Debug, Clone)]
pub struct LiveEvent {
    pub event_type: String,
    pub json: String,
}

async fn send_event(event: LiveEvent, sse_channels: &SseChannels) -> anyhow::Result<()> {
    use async_std::sync::TrySendError;
    let event = Arc::new(event);
    let mut disconnected: Vec<usize> = Vec::new();
    {
        let channels = sse_channels.read().await;
        for (i, chan) in channels.iter().enumerate() {
            // chan.send(event.clone()).await;
            
            match chan.try_send(event.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    disconnected.push(i);
                }
            }
        }
    }
    if !disconnected.is_empty() {
        let mut channels = sse_channels.write().await;
        for i in disconnected.into_iter().rev() {
            channels.remove(i);
        }
    }
    
    Ok(())
}

async fn check_list(
    active: &HashSet<isize>,
    current: &[Live],
    sse_channels: &SseChannels,
    event_types: (&str, &str),
) -> anyhow::Result<()> {
    for live in current.iter() {
        if !active.contains(&live.id) {
            let json = serde_json::to_string(live)?;
            
            let event = LiveEvent {
                event_type: event_types.0.into(),
                json,
            };
            send_event(event, sse_channels).await?;
        }
    }
    for live_id in active.iter() {
        if !current.iter().any(|x| x.id == *live_id) {
            let event = LiveEvent {
                event_type: event_types.1.into(),
                json: live_id.to_string(),
            };
            send_event(event, sse_channels).await?;
        }
    }
    
    Ok(())
}

pub async fn auto_live_task(sse_channels: SseChannels, active: &mut HashSet<isize>, upcoming: &mut HashSet<isize>, mock: bool) -> anyhow::Result<()> {
    
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
                    let start = ((col as isize) - 5).max(0) as usize;
                    let end = (col + 500).min(line.len());
                    
                    fn find_char_boundary(s: &str, i: usize) -> usize {
                        let mut bound = i;
                        while !s.is_char_boundary(bound) {
                            bound -= 1;
                        }
                        bound
                    }
                    let start = find_char_boundary(line, start);
                    let end = find_char_boundary(line, end);
                    
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
        
        check_list(
            &active,
            &data.live,
            &sse_channels,
            ("live", "unlive"),
        ).await?;
        check_list(
            &upcoming,
            &data.upcoming,
            &sse_channels,
            ("upcoming_add", "upcoming_remove"),
        ).await?;
        
        *active = data.live.iter().map(|x| x.id).collect();
        *upcoming = data.upcoming.iter().map(|x| x.id).collect();
        
        async_std::task::sleep(::std::time::Duration::new(25, 0)).await;
    }
}
