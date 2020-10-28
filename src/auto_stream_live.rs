
use std::collections::HashSet;
use std::time::Duration;
use serde::{ Serialize, Deserialize };
use anyhow::Context as _;
use async_std::sync::Arc;
use async_std::future::timeout;
use crate::{ SseChannels, Current };

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveChannel {
    id: isize,
    yt_channel_id: Option<String>,
    bb_space_id: Option<String>,
    name: String,
    // description: Option<String>,
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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct JetriLive {
    live: Vec<Live>,
    upcoming: Vec<Live>,
    ended: Vec<Live>,
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

pub struct LiveTaskState {
    current: Current,
    active: HashSet<isize>,
    upcoming: HashSet<isize>,
    ended: HashSet<isize>,
}

impl LiveTaskState {
    pub fn new(current: Current) -> Self {
        Self {
            current,
            active: HashSet::new(),
            upcoming: HashSet::new(),
            ended: HashSet::new(),
        }
    }
}

pub async fn auto_live_task(sse_channels: SseChannels, state: &mut LiveTaskState, mock: bool) -> anyhow::Result<()> {
    
    let ref mut current = state.current;
    let ref mut active = state.active;
    let ref mut upcoming = state.upcoming;
    let ref mut ended = state.ended;
    
    if mock {
        async_std::task::sleep(::std::time::Duration::new(5, 0)).await;
    }
    
    loop {
        
        let body: String = if mock {
            active.clear();
            let body = async_std::fs::read_to_string("./mock_data.json").await?;
            body
        } else {
            let mut res = timeout(
                    Duration::from_secs(15),
                    surf::get("https://api.holotools.app/v1/live?max_upcoming_hours=2190")
                ).await?
                .map_err(|err| anyhow::anyhow!(err))?;
            let body: String = timeout(
                    Duration::from_secs(15),
                    res.body_string()
                ).await?
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
        
        {
            // hold a lock on current so a client can't recieve an initial state
            // needing an update that was already sent
            let mut current = current.write().await;
            
            check_list(
                &active,
                &data.live,
                &sse_channels,
                ("live_add", "live_rem"),
            ).await?;
            check_list(
                &upcoming,
                &data.upcoming,
                &sse_channels,
                ("upcoming_add", "upcoming_rem"),
            ).await?;
            check_list(
                &ended,
                &data.ended,
                &sse_channels,
                ("ended_add", "ended_rem"),
            ).await?;
            
            *active = data.live.iter().map(|x| x.id).collect();
            *upcoming = data.upcoming.iter().map(|x| x.id).collect();
            *ended = data.ended.iter().map(|x| x.id).collect();
            
            *current = data;
        }
        
        async_std::task::sleep(::std::time::Duration::new(25, 0)).await;
    }
}
