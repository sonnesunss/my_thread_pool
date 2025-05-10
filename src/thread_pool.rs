/*
   实现自己的线程池
*/

use crossbeam::channel;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

#[cfg(feature = "debug_logs")]
macro_rules! log {
    ($($arg:tt)*) => {
        println!("-> Debug Info: {}", format!($($arg)*));
    };
}

#[cfg(not(feature = "debug_logs"))]
macro_rules! log {
    ($($arg:tt)*) => {};
}

type Job = Box<dyn FnOnce() + 'static + Send>;
// 线程池内允许的最大job数量的默认值
const MAX_QUEUE_SIZE: usize = 100;

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
    fn new(id: usize, receiver: Arc<channel::Receiver<Message>>) -> Worker {
        // 创建线程，其内循环不主动退出
        // spawn函数会返回一个JoinHandle, 需要这个返回值等待线程的安全退出，在Drop资源时使用
        let t = thread::Builder::new()
            .name(format!("worker-{}", id))
            .spawn(move || {
                let _thread_name = thread::current().name().unwrap_or("Unnamed thread").to_string();
                log!("[{}] Started", _thread_name);
            loop {
                // 从channel中接收消息, 这里会blocking wait阻塞等待，不会产生忙等待busy waiting
                // 没有job通过channel进来时线程会被挂起，等到job来临且消费那么当前线程会被唤醒
                match receiver.recv() {
                    Ok(Message::NewJob(job)) => {
                        log!("do job from worker[{}]", id);
                        let result = catch_unwind(AssertUnwindSafe(job));
                        if let Err(e) = result {
                            eprintln!("Job panicked in worker[{}] with error {:?}, but thread continue.", id, e);
                        }
                    }
                    // 如果是byebye信号就退出线程
                    Ok(Message::ByeBye) => {
                        log!("ByeBye from worker[{}]", id);
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
    workers: Vec<Worker>,                     // Worker array
    max_workers: usize,                       // max threads
    sender: Option<channel::Sender<Message>>, // channel's sender side
}

#[allow(dead_code)]
impl ThreadPool {
    /// 参数为最大线程数
    pub fn new(max_workers: usize) -> Self {
        if max_workers == 0 {
            panic!("max_workers must be greater than zero!")
        }

        // 创建一个channel，使用它传递具体要执行的任务
        let (tx, rx) = channel::bounded(MAX_QUEUE_SIZE);

        let mut workers = Vec::with_capacity(max_workers);
        let receiver = Arc::new(rx);

        for i in 0..max_workers {
            workers.push(Worker::new(i, Arc::clone(&receiver)));
        }

        Self {
            workers: workers,
            max_workers: max_workers,
            sender: Some(tx),
        }
    }

    /// 发送一个任务给线程池执行
    // 返回一个值，让调用方知道是否成功
    pub fn execute<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce() + 'static + Send, // -> 这个约束是线程闭包的约束
    {
        let job = Message::NewJob(Box::new(f));

        match &self.sender {
            Some(sender) => sender.send(job).map_err(|e| format!("Send failed: {}", e)),
            None => Err("ThreadPool is already shut down.".to_string()),
        }
    }

    /// 显式关闭线程池
    pub fn shutdown(&mut self) {
        if let Some(sender) = self.sender.take() {
            log!("ThreadPool Shutting down...");

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

            log!("ThreadPool Shutdown complete");
        } else {
            log!("ThreadPool Shutdown already called.");
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ThreadPool {
    /// 线程池执行job并返回结果
    pub fn execute_with_result<F, R>(&self, f: F) -> channel::Receiver<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = channel::bounded(1); // bounded(1) 表示一个单值返回通道

        let job = Box::new(move || {
            let result = f();
            let _ = tx.send(result);
        });

        // 任务放进线程池中
        if let Some(sender) = &self.sender {
            let _ = sender.send(Message::NewJob(job));
        }

        rx
    }
}

/////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn it_works() {
        let p = ThreadPool::new(5);
        let _ = p.execute(|| println!("do new job1"));
        match p.execute(|| println!("do new job2")) {
            Ok(r) => println!("{:?}", r),
            Err(e) => eprintln!("{e}"),
        }
        let _ = p.execute(|| println!("do new job3"));
        let _ = p.execute(|| println!("do new job4"));
        let _ = p.execute(|| println!("do new job5"));
    }

    #[test]
    fn test_shutdown() {
        let mut p = ThreadPool::new(5);
        let _ = p.execute(|| println!("do new job1"));
        let _ = p.execute(|| println!("do new job2"));
        let _ = p.execute(|| println!("do new job3"));
        let _ = p.execute(|| println!("do new job4"));
        p.shutdown();
        let _ = p.execute(|| println!("do new job5"));
    }

    #[test]
    fn test_execute_after_shutdown_fails() {
        let mut pool = ThreadPool::new(2);
        pool.shutdown();
        let res = pool.execute(|| {
            println!("This should not run");
        });

        assert!(res.is_err());
    }

    #[test]
    fn test_execute_with_result_basic() {
        let pool = ThreadPool::new(4);

        let rx = pool.execute_with_result(|| 21 * 2);

        let result = rx.recv().unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_execute_with_result_string() {
        let pool = ThreadPool::new(2);

        let rx = pool.execute_with_result(|| "hello".to_string());

        let result = rx.recv().unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_multiple_execute_with_result() {
        let pool = ThreadPool::new(4);

        let handles: Vec<_> = (0..10)
            .map(|i| pool.execute_with_result(move || i * 2))
            .collect();

        let mut results: Vec<_> = handles.into_iter().map(|rx| rx.recv().unwrap()).collect();
        results.sort();
        assert_eq!(results, (0..10).map(|i| i * 2).collect::<Vec<_>>());
    }

    #[test]
    fn test_execute_with_result_after_shutdown() {
        let mut pool = ThreadPool::new(2);
        pool.shutdown();

        let rx = pool.execute_with_result(|| 123); // 应该不会真正执行任务

        // 因为线程池已经 shutdown，任务不会执行，所以 recv 会阻塞
        // 我们加个超时 recv 以防测试卡住
        let result = rx.recv_timeout(Duration::from_millis(100));
        assert!(result.is_err()); // 没有收到值
    }
}
