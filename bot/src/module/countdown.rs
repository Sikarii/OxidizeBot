use crate::{command, config, module, template, utils};
use futures::{sync::mpsc, Async, Future, Poll, Stream};
use std::{fs, path::PathBuf, time};
use tokio::timer;

enum Event {
    /// Set the countdown.
    Set(utils::Duration, template::Template),
    /// Clear the countdown.
    Clear,
}

pub struct Handler {
    sender: mpsc::UnboundedSender<Event>,
}

impl command::Handler for Handler {
    fn handle<'m>(&mut self, mut ctx: command::Context<'_, '_>) -> Result<(), failure::Error> {
        match ctx.next() {
            Some("set") => {
                ctx.check_moderator()?;

                let duration = ctx_try!(ctx.next_parse("<duration> <template>", "!countdown set"));
                let template = ctx_try!(ctx.rest_parse("<duration> <template>", "!countdown set"));

                match self.sender.unbounded_send(Event::Set(duration, template)) {
                    Ok(()) => {
                        ctx.respond("Countdown set!");
                    }
                    Err(_) => {
                        ctx.respond("Could not set countdown :(");
                        return Ok(());
                    }
                }
            }
            Some("clear") => {
                ctx.check_moderator()?;

                match self.sender.unbounded_send(Event::Clear) {
                    Ok(()) => {
                        ctx.respond("Countdown cleared!");
                    }
                    Err(_) => {
                        ctx.respond("Could not clear countdown :(");
                        return Ok(());
                    }
                }
            }
            _ => {
                ctx.respond("Expected: !countdown set <duration> <template>, or !countdown clear");
                return Ok(());
            }
        }

        Ok(())
    }
}

pub struct Module {
    path: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    path: PathBuf,
}

impl Module {
    pub fn load(_config: &config::Config, module: &Config) -> Result<Self, failure::Error> {
        Ok(Module {
            path: module.path.clone(),
        })
    }
}

impl super::Module for Module {
    fn ty(&self) -> &'static str {
        "countdown"
    }

    /// Set up command handlers for this module.
    fn hook(
        &self,
        module::HookContext {
            handlers, futures, ..
        }: module::HookContext<'_>,
    ) -> Result<(), failure::Error> {
        let (sender, receiver) = mpsc::unbounded();

        handlers.insert("countdown", Handler { sender });

        futures.push(Box::new(CountdownFuture {
            receiver,
            path: self.path.clone(),
            current: None,
        }));

        Ok(())
    }
}

struct CountdownFuture {
    receiver: mpsc::UnboundedReceiver<Event>,
    path: PathBuf,
    current: Option<Current>,
}

struct Current {
    duration: utils::Duration,
    template: template::Template,
    elapsed: utils::Duration,
    interval: timer::Interval,
    path: PathBuf,
}

impl Current {
    fn write(&mut self) -> Result<(), failure::Error> {
        let mut f = fs::File::create(&self.path)?;
        let remaining = self.duration.saturating_sub(self.elapsed.clone());
        let remaining = remaining.as_digital();
        let elapsed = self.elapsed.as_digital();
        let duration = self.duration.as_digital();

        self.template.render(
            &mut f,
            Data {
                remaining,
                elapsed,
                duration,
            },
        )?;

        return Ok(());

        #[derive(serde::Serialize)]
        struct Data {
            remaining: String,
            elapsed: String,
            duration: String,
        }
    }

    fn clear(&mut self) -> Result<(), failure::Error> {
        if !self.path.is_file() {
            return Ok(());
        }

        fs::remove_file(&self.path)?;
        Ok(())
    }

    /// Attempt to write an update and log on errors.
    fn write_log(&mut self) {
        if let Err(e) = self.write() {
            log_err!(e, "failed to write: {}", self.path.display());
        }
    }

    /// Attempt to clear the file and log on errors.
    fn clear_log(&mut self) {
        if let Err(e) = self.clear() {
            log_err!(e, "failed to clear: {}", self.path.display());
        }
    }
}

impl Stream for Current {
    type Item = ();
    type Error = failure::Error;

    fn poll(&mut self) -> Poll<Option<()>, failure::Error> {
        if let Some(_) = try_infinite!(self.interval.poll()) {
            self.elapsed += utils::Duration::seconds(1);

            if self.elapsed >= self.duration {
                return Ok(Async::Ready(None));
            }

            self.write()?;
            return Ok(Async::Ready(Some(())));
        }

        Ok(Async::NotReady)
    }
}

impl Future for CountdownFuture {
    type Item = ();
    type Error = failure::Error;

    fn poll(&mut self) -> Poll<(), failure::Error> {
        loop {
            let mut not_ready = true;

            if let Some(e) = try_infinite_empty!(self.receiver.poll()) {
                match e {
                    Event::Set(duration, template) => {
                        let mut current = Current {
                            duration,
                            template,
                            elapsed: Default::default(),
                            interval: timer::Interval::new_interval(time::Duration::from_secs(1)),
                            path: self.path.clone(),
                        };

                        current.write_log();
                        self.current = Some(current);
                    }
                    Event::Clear => {
                        if let Some(mut current) = self.current.take() {
                            current.clear_log();
                        }
                    }
                }

                not_ready = false;
            }

            if let Some(current) = self.current.as_mut() {
                match current.poll()? {
                    Async::Ready(None) => {
                        current.clear_log();
                        self.current = None;
                    }
                    Async::Ready(Some(())) => not_ready = false,
                    Async::NotReady => (),
                }
            }

            if not_ready {
                return Ok(Async::NotReady);
            }
        }
    }
}
