mod thread_pool;

use std::thread;
use std::time::Duration;
use thread_pool::ThreadPool;

fn main() {
    println!("Hello, world!");

    let f1 = || {
        let dur = Duration::from_secs(1);
        thread::sleep(dur);
        println!("do new job2");
    };

    let mut p = ThreadPool::new(8);

    let _ = p.execute(|| println!("do new job1"));
    let _ = p.execute(f1);
    let _ = p.execute(|| println!("do new job3"));
    let _ = p.execute(|| println!("do new job4"));
    let _ = p.execute(|| println!("do new job5"));
    let _ = p.execute(|| println!("do new job6"));
    let _ = p.execute(|| println!("do new job7"));
    let _ = p.execute(|| println!("do new job8"));

    p.shutdown();

    let _ = p.execute(f1);
}
