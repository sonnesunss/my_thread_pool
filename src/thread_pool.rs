/*
   实现自己的线程池
*/

use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + 'static + Send>;

// 定义可以通过mpsc channel发送的msg类型
enum Message {
    ByeBye, // 表示可以退出, drop时使用，发送这个case给worker告知worker可以退出，也就是线程退出
    NewJob(Job), // 表示一个新任务
}

// 对可以发往线程池内执行的任务的抽象
//
// 一个worker 包含worker编号 + 线程
//
struct Worker {
    _id: usize,                // worker num
    t: Option<JoinHandle<()>>, // 持有线程JoinHandle
}

#[allow(dead_code)]
impl Worker {
    // 传递线程id、channel's receiver side
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
        // 创建线程，其内循环不主动退出
        // spawn函数会返回一个JoinHandle, 需要这个返回值等待线程的安全退出，在Drop资源时使用
        let t = thread::Builder::new()
            .name(format!("worker-{}", id))
            .spawn(move || {
                let thread_name = thread::current().name().unwrap_or("Unnamed thread").to_string();
                println!("[{}] Started", thread_name);
            loop {
                // 接收发送来的任务
                // 这里的实现略显粗糙，unwrap可能会造成程序的panic退出
                // let message = receiver.lock().unwrap().recv().unwrap();

                // 首先应该尝试获取锁
                let lock = match receiver.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        eprintln!(
                            "Worker [{id}] failed to acquire lock due to poisoning. Attempting to recover."
                        );
                        poisoned.into_inner() // 尝试恢复，这是被rust所允许的，尽管知道风险但是还是愿意自己承担风险使用中毒后的值
                    }
                };
                // 然后再尝试从channel中接收消息
                match lock.recv() {
                    Ok(Message::NewJob(job)) => {
                        println!("do job from worker[{}]", id);
                        job();
                    }
                    // 如果是byebye信号就退出线程
                    Ok(Message::ByeBye) => {
                        println!("ByeBye from worker[{}]", id);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Worker [{id}] channel disconnected: {e}");
                        break;
                    }
                }
            }
        }).expect("Failed to spawn thread");

        Worker {
            _id: id,
            t: Some(t),
        }
    }
}

// 定义线程池
// 1. 一个显然的字段是需要定义一个表示最大线程数目的字段
// 2. 定义工作线程数组，表示具体的工作者族群
// 3. 如何给线程池内的线程发送一段工作逻辑呢？ 使用mpsc Channel是一个不错的选择，mpsc channel是共享、广播的
pub struct ThreadPool {
    workers: Vec<Worker>,                  // Worker array
    max_workers: usize,                    // max threads
    sender: Option<mpsc::Sender<Message>>, // channel's sender side
}

#[allow(dead_code)]
impl ThreadPool {
    pub fn new(max_workers: usize) -> Self {
        if max_workers == 0 {
            panic!("max_workers must be greater than zero!")
        }

        // 创建一个channel， 使用它传递具体要执行的任务
        let (tx, rx) = mpsc::channel();

        let mut workers = Vec::with_capacity(max_workers);
        let receiver = Arc::new(Mutex::new(rx));

        for i in 0..max_workers {
            workers.push(Worker::new(i, Arc::clone(&receiver)));
        }

        Self {
            workers: workers,
            max_workers: max_workers,
            sender: Some(tx),
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + 'static + Send, // -> 这个约束是线程闭包的约束
    {
        let job = Message::NewJob(Box::new(f));
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.send(job) {
                eprintln!("ThreadPool failed to send job: {}", e);
            }
        }
        // self.sender.send(job).unwrap();
    }

    pub fn shutdown(&mut self) {
        if let Some(sender) = self.sender.take() {
            println!("ThreadPool Shutting down...");

            // 向所有线程发送byebye
            // 先退出所有的线程, 必须发送等同于max_workers数量的Message::ByeBye才可以让所有线程都可以安全的退出
            // 因为channel是共享的，是广播给所有的，但是每一次的发送都只能被一个获取到的线程所消费掉，一次发送并不能让所有的线程获得数据
            // 这是需要特别注意的
            for _ in 0..self.max_workers {
                let _ = sender.send(Message::ByeBye);
            }

            // 等待所有线程安全退出
            // 等待所有的线程安全退出后，每个线程都会返回JoinHandle
            // 使用take取出Some，并在原位置放置一个None值, 这是为了防止double-join 造成死锁
            // join是阻塞调用的，能够确保对应的线程真正的结束
            for worker in self.workers.iter_mut() {
                if let Some(handle) = worker.t.take() {
                    let _ = handle.join();
                }
            }

            println!("ThreadPool Shutdown complete");
        } else {
            println!("ThreadPool Shutdown already called.");
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let p = ThreadPool::new(5);
        p.execute(|| println!("do new job1"));
        p.execute(|| println!("do new job2"));
        p.execute(|| println!("do new job3"));
        p.execute(|| println!("do new job4"));
        p.execute(|| println!("do new job5"));
    }

    fn test_shutdown() {
        let mut p = ThreadPool::new(5);
        p.execute(|| println!("do new job1"));
        p.execute(|| println!("do new job2"));
        p.execute(|| println!("do new job3"));
        p.execute(|| println!("do new job4"));
        p.shutdown();
        p.execute(|| println!("do new job5"));
    }
}
