rust_chat
=


Multithreaded chat server implemented in rust.


## INSTALL

```
git clone https://github.com/jt-l/rust_chat.git
```

## RUN

```
cargo run  (optionally run with a port as an argument)
```

## BUILD

```
cargo build --release
```

## DOCS

```
cargo doc
```

## DESIGN 

The high-level design of the system is as follows: 

The main thread handles incoming connections and spawns a child thread for each client that connects.

There is one shared data structure which is a hashmap that maps chatroom names to lists of client connections (TCP streams). This data structure is protected by a mutex and is using rust's ARC data type (ARC = Automatically referenced counted) -- a thread safe reference counting pointer. 

When a client types a message, joins, or disconnects from a chat room the thread for which that client is running in attempts to aquire the mutex. Once the mutex
is aquired the client thread will query, delete, or update the shared data store and write to each other client the desired message which corresponds to the action the client is taking (i.e. leaving chat, sending a message, etc). 

## IMPROVEMENTS

Here is a list of potential improvements that could be made to the code to increase quality. 

1. Unit tests
2. Code refactoring -- split logic into more functions/more code reuse/handle errors better (e.g. clean up use of unwrap() calls)/more idiomatic rust code style/clean up comments
3. Performance testing -- write a script to generate load on the system; record # of concurrent users, response time, cpu utilization, and memory utilization. This information
can be used to find bottlenecks and then the code can be optimized accordingly.
