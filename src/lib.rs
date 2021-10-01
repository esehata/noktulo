pub mod kad;
pub mod crypto;
pub mod user;
pub mod util;
pub mod service;
pub mod cli;

#[cfg(test)]
mod tests {
    use chrono::Local;

    #[test]
    fn test (){
        let now = Local::now();
        println!("{}",now);
    }
}