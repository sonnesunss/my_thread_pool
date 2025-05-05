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
    _id: usize, // worker num
    t: Option<JoinHandle<()>>,
}

#[allow(dead_code)]
impl Worker {
    // 传递线程id、channel's receiver side
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
        // 创建线程，其内循环不主动退出
        // spawn函数会返回一个JoinHandle
        let t = thread::spawn(move || {
            loop {
                // 接收发送来的任务
                // 这里的实现略显粗糙，unwrap会造成程序的panic退出
                let message = receiver.lock().unwrap().recv().unwrap();
                // 使用模式匹配 匹配所有分支情况
                match message {
                    // 如果传递进来的是新任务就执行
                    Message::NewJob(job) => {
                        println!("do job from worker[{}]", id);
                        job();
                    }
                    // 如果是byebye信号就退出线程
                    Message::ByeBye => {
                        println!("ByeBye from worker[{}]", id);
                        break;
                    }
                }
            }
        });

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
    workers: Vec<Worker>,          // Worker array
    max_workers: usize,            // max threads
    sender: mpsc::Sender<Message>, // channel's sender side
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
            sender: tx,
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + 'static + Send, // -> 这个约束是线程闭包的约束
    {
        let job = Message::NewJob(Box::new(f));
        self.sender.send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        // 先退出所有的线程, 必须发送等同于max_workers数量的Message::ByeBye才可以让所有线程都可以安全的退出
        // 因为channel是共享的，是广播给所有的，但是每一次的发送都只能被一个获取到的线程所消费掉，一次发送并不能让所有的线程获得数据
        // 这是需要特别注意的
        for _ in 0..self.max_workers {
            self.sender.send(Message::ByeBye).unwrap();
        }
        // 等待所有的线程安全退出后，每个线程都会返回JoinHandle
        // 使用take取出Some，并在原位置放置一个None值, 这是为了防止double-join 造成死锁
        // join是阻塞调用的，能够确保对应的线程真正的结束
        for w in self.workers.iter_mut() {
            if let Some(t) = w.t.take() {
                t.join().unwrap();
            }
        }
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
}
