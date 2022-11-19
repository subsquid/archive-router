
pub trait Fs {
    fn ls(&self, path: &str) -> Result<Vec<String>, String>;
    fn abs(&self, path: &str) -> String;
}
