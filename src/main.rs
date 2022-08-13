use std::{env, io::{Error, Read}, time::Duration, borrow::Cow, path::{PathBuf, Path}, pin::Pin, sync::{mpsc::channel, Arc, Mutex, RwLock}, cell::RefCell};

use futures::task::Spawn;
use futures_util::{future, StreamExt, TryStreamExt, SinkExt, Future, FutureExt};
use lazy_static::lazy_static;
use log::debug;
use notify::{Watcher, RecursiveMode, ReadDirectoryChangesWatcher};
use serde_json::Value;
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncWriteExt, self, AsyncReadExt}, sync::broadcast};
use tokio_tungstenite::tungstenite::{Message, error::ProtocolError, protocol::{CloseFrame, frame::coding::CloseCode}};
use serde::{Serialize, Deserialize, Deserializer};
use async_recursion::async_recursion;

lazy_static!{
    static ref WATCHER : RwLock<Option<ReadDirectoryChangesWatcher>> = RwLock::new(None);
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let addr = "0.0.0.0:1111";

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    let (tx, mut rx1) = broadcast::channel(16);

    let txref = RefCell::new(tx);
    let txref2 = txref.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        match res {
            Ok(event) => {
                println!("we {:?}", event);
                for path in event.paths {
                    txref2.borrow().send(path.to_str().unwrap().to_string()).unwrap();
                }
            }
            Err(e) => {
                println!("Watch error: {:?}", e);
            }
        }
    }).unwrap();

    WATCHER.write().unwrap().replace(watcher);
    let txref3 = txref.clone();

    while let Ok((stream, _)) = listener.accept().await {
        let mut sub = txref3.borrow().subscribe();
        tokio::spawn(accept_connection(stream,sub));
    }

    Ok(())
}
#[derive(Debug,Serialize, Deserialize,Clone)]
enum DataTypes {
    String(String),
    Object(serde_json::Value),
    FsObject(WsDataFs)
}


#[derive(Debug,Serialize, Deserialize,Clone)]
struct WsData {
    c: i32,
    d: DataTypes
}
#[derive(Debug,Serialize, Deserialize,Clone)]
struct WsDataFs {
    path: String,
    ftype:i8,
    content:String
}

#[async_recursion]
async fn create_files(obj:Value,path:PathBuf) {
    if let Value::Object(arr) = obj {
        for (k,item) in arr {
            match &item {
                Value::String(s) => {
                    let f = tokio::fs::File::create(path.join(&k)).await.unwrap();
                }
                Value::Object(obj) => {
                    let pathx = path.join(&k);
                    tokio::fs::create_dir_all(&pathx).await;
                    create_files(Value::Object(obj.to_owned()),pathx).await;
                }
                _=>{}
            }
        }
    }
}

async fn accept_connection(stream: TcpStream,mut sub: broadcast::Receiver<String>) {
    let addr = stream.peer_addr().expect("connected streams should have a peer address");
    println!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    println!("New WebSocket connection: {}", addr);
    let connecting_ip = addr.ip().to_string();

    let (mut write, mut read) = ws_stream.split();
    // We should not forward messages other than text or binary.
    write.send(Message::text("pog")).await;

    let mut interval = tokio::time::interval(Duration::from_millis(3000));
    let mut recived_pong = false;
    let mut strikes = 0;
    
    let mut computer_id = String::new();


    let heartbeat_ping = serde_json::to_string(&WsData{
        c:9,
        d:DataTypes::String("ping".to_string())
    }).unwrap();

    'pog: loop {
        tokio::select! {
            subsc = sub.recv() => {
                match subsc {
                    Ok(s) => {
                        if s.contains(&connecting_ip) && s.contains(&computer_id) {
                            let search_val = format!("./servers/{}#{}",&connecting_ip,&computer_id);
                            if let Some(idx) = s.find(&search_val) {
                                match std::fs::File::open(&s) {
                                    Ok(mut f) => {
                                        
                                        let mut contents = String::new();
                                        f.read_to_string(&mut contents).unwrap();
                                        let slice = &s[idx+search_val.len()..];
                                        let cc_path = slice.replace('\\',"/");
                                        let wsMsg = WsData {
                                            c:1,
                                            d:DataTypes::FsObject(WsDataFs {
                                                path:cc_path.to_string(),
                                                ftype:0,
                                                content:contents
                                            })
                                        };
                                        println!("sending sync update");
                                        write.send(Message::text(serde_json::to_string(&wsMsg).unwrap())).await.unwrap();
                                    },
                                    Err(e) =>{
                                        println!("unable to open file {:?}",e);
                                    }
                                }
                            }

                        }
                    },
                    Err(_) => {
                        println!("Error in sub");
                    },
                }
            }
            msg = read.next() => {
                match msg {
                    Some(msg) => {
                        match msg {
                            Err(ProtocolError) => {
                                println!("Protocol error, disconnecting");
                                break;
                            },
                            Ok(Message::Text(text)) => {
                                debug!("Received text: {}", text);
                                let jsn = serde_json::from_str::<WsData>(&text);
                                match jsn{
                                    Ok(json) => {
                                        debug!("read data = {:?}", &json);
                                        match &json.c {
                                            0 => {
                                                if let DataTypes::String(id) = json.d {
                                                    let p = format!("./servers/{}#{}",&connecting_ip,&id);
                                                    tokio::fs::create_dir(&p).await;
                                                    computer_id = id;
                                                    println!("connected computer id: {}", &computer_id);
                                                }
                                            },
                                            1 => {
                                                if let DataTypes::FsObject(obj) = json.d {
                                                    if obj.ftype == 0 {
                                                        let path = PathBuf::from(format!("./servers/{}#{}",&connecting_ip,&computer_id)).join(format!(".{}",&obj.path));
                                                        println!("🔄️ {:?}",path);
                                                        let mut f =  tokio::fs::File::create(path).await.unwrap();
                                                        f.write_all(obj.content.as_bytes()).await;
                                                        // file_operations_await.push(f.write_all(obj.content.as_bytes()).boxed());
                                                    } else {
                                                        let path = PathBuf::from(format!("./servers/{}#{}",&connecting_ip,&computer_id)).join(format!(".{}",&obj.path));
                                                        tokio::fs::create_dir_all(path).await;
                                                    }
                                                    // create_files(obj.clone(),path).await;
                                                    // create_files(obj,PathBuf::from(format!("./{}",))).await;
                                                }
                                            },
                                            2 => {
                                                let p = format!("./servers/{}#{}",&connecting_ip,&computer_id);
                                                WATCHER.write().unwrap().as_mut().unwrap().watch(Path::new(&p),RecursiveMode::Recursive);
                                                println!("sync complete. starting to watch for updates.")
                                            }
                                            9 => {
                                                debug!("Received pong");
                                                recived_pong = true;
                                                strikes = 0;
                                            }
                                            _=>debug!("unknown code")
                                        }
                                    }
                                    Err(e) => {
                                        debug!("{:?}", &e);
                                        println!("something went wrong");
                                        write.send(Message::Close(Some(CloseFrame{code:CloseCode::Invalid,reason:Cow::Owned(format!("{:?}",&e))}))).await;
                                        break 'pog;
                                    }
                                }
                                
                            }
                            _ => {
                                debug!("Received other message");
                            }
                        }
                    },
                    None => break 'pog,
                }
            }
            _ = interval.tick() => {
                if !recived_pong {
                    strikes+=1;
                    if strikes > 3 {
                        println!("Too many strikes, disconnecting");
                        break 'pog;
                    }
                }
                debug!("Ping");
                write.send(Message::Text(heartbeat_ping.clone())).await.unwrap();
            }
        }
        // match read.next().await {
            
        // }

    }

    println!("Peer [{}] dc", addr);
    WATCHER.write().unwrap().as_mut().unwrap().unwatch(Path::new(&format!("./servers/{}#{}",&connecting_ip,&computer_id)));
    write.send(Message::Close(None)).await;
}