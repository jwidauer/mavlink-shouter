use log::error;

pub trait LogError<T> {
    fn log_error(self) -> Option<T>;

    fn log(e: impl std::fmt::Display) {
        error!("{}", e);
    }
}

impl<T, E: std::fmt::Display> LogError<T> for Result<T, E> {
    fn log_error(self) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                Self::log(e);
                None
            }
        }
    }
}
