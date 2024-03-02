use log::error;

pub trait LogError<T> {
    fn log_error(self) -> Option<T>;

    fn log(e: &impl std::error::Error) {
        let mut msg = e.to_string();
        let mut e = e as &dyn std::error::Error;
        while let Some(source) = e.source() {
            msg += &format!(" caused by: {}", source);
            e = source;
        }
        error!("{}", msg);
    }
}

impl<T, E: std::error::Error> LogError<T> for Result<T, E> {
    fn log_error(self) -> Option<T> {
        self.inspect_err(Self::log).ok()
    }
}
