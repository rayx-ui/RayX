use crate::configuration::ConfigurationBuilder;
use crate::di::ServiceProvider;

pub trait AppBuilder: Sized {
    fn configuration_mut(&mut self) -> &mut ConfigurationBuilder;
    fn services_mut(&mut self) -> &mut ServiceProvider;
}
