use std::net::{TcpListener, TcpStream, Shutdown, SocketAddr};
use std::io::prelude::*;
use std::io::BufReader;
use std::thread;
use std::collections::HashMap;
use regex::Regex;
use std::sync::{Arc, Mutex};
use std::env;

const MAX_MESSAGE_LENGTH_IN_BYTES: usize = 20000;
const MAX_ROOM_NAME_LENGTH_IN_BYTES: usize = 20;
const MAX_USERNAME_LENGTH_IN_BYTES: usize = 20;
const INTERNAL_SERVER_ERROR: i32 = -1;
const INVALID_CLI_ARG: i32 = -2; 
const MAX_VALID_PORT: i32 = 49151; 

// struct to parse the clients messages into 
#[derive(Debug)]
pub enum Command {
    Join { room_name: String, username: String},
    Message { message: String }
}

// struct to hold client state
#[derive(Debug)]
struct ClientState(String, String, SocketAddr);

fn main() {

    // set default server port
    let mut port = "1234".to_string();

    // final addr to be combined with the port and used to start the server
    let mut addr = "127.0.0.1:".to_string();

    // attempt to parse port command line arg
    match env::args().nth(1) {
        Some(arg) => {
            // attempt to parse the argument into an int
            match arg.parse::<i32>() {
                Ok(int_arg) => {
                    // we have an int, check if it is in the valid port range
                    if int_arg > 0 && int_arg < MAX_VALID_PORT {
                        port = arg;
                    } else {
                        // invalid port
                        println!("Invalid command line argument. Input must be a valid port.");
                        std::process::exit(INVALID_CLI_ARG);
                    }
                }
                _ => {
                    // arg cannot be parsed to an int
                    println!("Invalid command line argument. Input must be a valid port.");
                    std::process::exit(INVALID_CLI_ARG);
                }
            }
        },
        _ => { 
            // no 1st arg, continue
        }
    }

    // set the addr 
    addr.push_str(&port);

    // data structure to hold chatrooms and clients connected to the chatrooms
    // chatroom name -> vec of ip addresses, where each ip in the vec is a client connected to that
    // chatroom
    let m: HashMap<String, Vec<(TcpStream, SocketAddr)>> = HashMap::new();

    // Using an arc to share the memory between threads, the data inside the arc is protected by a
    // mutex. ARC = Automatically reference counted -> a thread safe reference counting pointer
    let chatrooms = Arc::new(Mutex::new(m));

    // start server
    let listener = TcpListener::bind(addr).unwrap();

    // vec to hold the children which are spawned
    let mut children = vec![];

    loop {

        // listener.accept() blocks
        // accept connections and process, each connection is its own thread
        match listener.accept() {
            Ok((stream, addr)) => {

                let chatrooms = Arc::clone(&chatrooms);

                children.push(thread::spawn(move || {
                    handle_client(stream, chatrooms, addr);
                }));
            }
                Err(e) => println!("couldn't get client: {:?}", e),
        }
    }
}

// Handles each connected client
// updates server state / chatroom state
// The shared state can only be accessed once the lock is held.
fn handle_client(stream: TcpStream, chatrooms: Arc<Mutex<HashMap<String, Vec<(TcpStream, SocketAddr)>>>>, addr: SocketAddr) {

    // stores client chatroom and client username 
    let mut client_state = ClientState("".to_string(), "".to_string(), addr);

    loop {

        // message to be recieved from a client
        let mut message = String::new();

        // create a reader from the TCP stream
        // try_clone creates a new independently owned handle to the underlying socket
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        // Receive a message from the client, read_line blocks 
        match reader.read_line(&mut message) {
            Ok(_success) => (),
            Err(_e) => {
                // reader failed to read, handle the client killing the connection
                // acquire mutex
                let mut chatrooms = chatrooms.lock().unwrap();

                // get the vec of all clients
                match chatrooms.get_mut(&client_state.0) {
                    Some(clients) => {

                        // find the index of the client to be removed
                        let mut pos_to_remove = 0;

                        // nightly build of rust has a remove_item function on vec; this could be
                        // done cleaner or use different data structure.
                        for (pos, client) in clients.iter().enumerate() {
                            if client.1 == client_state.2 {
                                pos_to_remove = pos;
                            }
                        }

                        // remove the client
                        clients.remove(pos_to_remove);

                        // write to the remaining clients the username that is leaving
                        for client in clients {

                            let mut stream = client.0.try_clone().unwrap();

                            // create message to send
                            let mut message_to_send = client_state.1.to_string();
                            let joined = " has left\n".to_string();
                            message_to_send.push_str(&joined);

                            // write to client
                            match stream.write(message_to_send.as_bytes()) {
                                Ok(_success) => continue,
                                Err(e) => {
                                    // write error to a client. Client may have closed the connection during a
                                    // write. Clean up
                                    println!("Write Error: {:?}", e);
                                    stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                    return
                                }
                            }
                        }
                        
                        // shutdown the connection 
                        stream.shutdown(Shutdown::Both).expect("shutdown call failed");                    
                    }
                    _ => {
                        // the user is not currently in a chatroom, shutdown connection
                        stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                    }
                }
                return 
            }
        }

        // if it is a valid message then transmit the message to the other clients in the chatroom
        client_state = match validate_message(&message) {
            Ok(command) => {
                match command {
                    Command::Join {room_name, username} => {
                        // acquire the mutex, mutex is unlocked when chatrooms goes out of scope
                        let mut chatrooms = chatrooms.lock().unwrap();
                        
                        // check if the chatroom already exists in chatrooms
                        if chatrooms.contains_key(&room_name) {
                            // the room already exists so insert the current client into the room
                            // get the current vec of clients with get_mut so that we can append
                            // the new client
                            match chatrooms.get_mut(&room_name) {
                                Some(clients) => {
                                    // create a new handle to the underlying socket,
                                    // this handle is to be used for writing and requires mutex access
                                    let writer = stream.try_clone().unwrap();

                                    // add the client to the vec of clients
                                    clients.push((writer, addr));

                                    // write the join message to all clients in the chat
                                    for client in clients {

                                        // create message to send
                                        let mut message_to_send = username.to_string();
                                        let joined = " has joined\n".to_string();
                                        message_to_send.push_str(&joined);

                                        // write to client
                                        match client.0.write(message_to_send.as_bytes()) {
                                            Ok(_success) => continue,
                                            Err(e) => {
                                                // write error to a client. Client may have closed the connection during a
                                                // write. Clean up
                                                println!("Write Error: {:?}", e);
                                                stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                                return
                                            }
                                        }
                                    }

                                    // update the current room for this client
                                    ClientState(room_name.clone(), username, addr)

                                }
                                _ => {
                                    // this should never happen... we just checked the hashmap
                                    // if it contains the room key and we have the lock
                                    // handling this to get rid of compiler warnings
                                    std::process::exit(INTERNAL_SERVER_ERROR);
                                }
                            }
                        } else {
                            // insert the room and add the client information to the vec
                            let mut clients: Vec<(TcpStream, SocketAddr)> = Vec::new(); 

                            // create a new handle to the underlying socket,
                            // this handle is to be used for writing and requires mutex access
                            clients.push((stream.try_clone().unwrap(), addr));

                            // create chatroom
                            chatrooms.insert(room_name.clone(), clients);

                            // create 'client has joined message'
                            let mut writer = stream.try_clone().unwrap();
                            let mut message_to_send = username.to_string();
                            let joined = " has joined\n".to_string();
                            message_to_send.push_str(&joined);

                            // write to client
                            match writer.write(message_to_send.as_bytes()) {
                                Ok(_success) => {
                                    ()
                                },
                                Err(e) => {
                                    // write error to a client. Client may have closed the connection during a
                                    // write. Clean up
                                    println!("Write Error: {:?}", e);
                                    stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                    return
                                }
                            }

                            // update the current room for this client
                            ClientState(room_name.clone(), username, addr)
                        }
                    }
                    Command::Message {message} => {

                        // acquire the mutex
                        let chatrooms = chatrooms.lock().unwrap();

                        match chatrooms.get(&client_state.0) {
                            Some(clients) => {
                                // write the message to all clients in the chat
                                for client in clients {

                                    let mut stream = client.0.try_clone().unwrap();

                                    // build message to send 
                                    let message_to_send = &mut client_state.1.to_string();
                                    let colon = ": ";
                                    message_to_send.push_str(&colon);
                                    message_to_send.push_str(&message);

                                    // write to client
                                    match stream.write(message_to_send.as_bytes()) {
                                        Ok(_success) => continue,
                                        Err(e) => {
                                            // write error to a client. Client may have closed the connection during a
                                            // write. Clean up
                                            println!("Write Error: {:?}", e);
                                            stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                            return                                            
                                        }
                                    }
                                }
                            }

                            // no active chatroom for the client
                            _ => {

                                // write an ERROR, since the client attempted to send a message
                                // before joining a chatroom
                                let mut writer = stream.try_clone().unwrap();

                                // write
                                match writer.write(b"ERROR\n") {
                                    Ok(_success) => (),
                                    Err(e) => { 
                                        // write error to a client. Client may have closed the connection during a
                                        // write. Clean up
                                        println!("Write Error: {:?}", e); 
                                        stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                        return
                                    }
                                }

                                // close the connection and return the thread
                                stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                                return 
                            }
                        }

                       client_state 
                    }
                }
            }
            // not a valid join command or a valid message (i.e. message exceeds 20k bytes) 
            _ => {

                // write back to the client, no need to acquire the mutex, since we just need to
                // send an error message to the sending client
                let mut writer = stream.try_clone().unwrap();

                // write
                match writer.write(b"ERROR\n") {
                    Ok(_success) => (),
                    Err(e) => { 
                        // write error to a client. Client may have closed the connection during a
                        // write. Clean up
                        println!("Write Error: {:?}", e);
                        stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                        return
                    }
                }

                // close the connection and return the thread
                stream.shutdown(Shutdown::Both).expect("shutdown call failed");
                return
            }
        }
    }
}


// Validate a message -- a valid message is either a Command:Join or a Command::Message 
fn validate_message(message: &str) -> Result<Command, &'static str> {

    // check if the message is less than 20k bytes in length
    // if it is then return an error
    let message_length = message.chars().count();
    if message_length > MAX_MESSAGE_LENGTH_IN_BYTES {
        return Err("Message Length is too long.")
    }

    // regex to parse a join message
    let re = Regex::new(r"(?i)(JOIN) (\w+) (\w+)").unwrap();


    // if the regex iter over the matches
    for cap in re.captures_iter(message) {

        // if the regex matches, but the length is not the same this means that it is an error
        // case (i.e. 'JOIN foo bar blah'). Not the best way to check for this. Can be cleaned up.
        let terminating_chars_length = 2;
        if cap[0].chars().count() != message_length - terminating_chars_length {
            return Err("Invalid join command.")
        }

        // validate that the room_name and username is within the specified bounds
        let room_name = cap[2].to_string();
        let username = cap[3].to_string();

        if room_name.chars().count() > MAX_ROOM_NAME_LENGTH_IN_BYTES || username.chars().count() > MAX_USERNAME_LENGTH_IN_BYTES {
            return Err("Invalid room_name or username length");
        } else {
            return Ok(Command::Join {room_name: room_name, username: username})
        }

    }

    // if no regex match and no other errors then the message is a valid message
    Ok(Command::Message {message: message.to_string()})
}
