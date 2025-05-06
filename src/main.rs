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
    p.execute(|| println!("do new job1"));
    p.execute(f1);
    p.execute(|| println!("do new job3"));
    p.execute(|| println!("do new job4"));
    p.execute(|| println!("do new job5"));
    p.execute(|| println!("do new job6"));
    p.execute(|| println!("do new job7"));
    p.execute(|| println!("do new job8"));

    p.shutdown();

    p.execute(f1);
}
