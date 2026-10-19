use super::*;

pub(super) struct PlayerIdle {
    pub hidden: bool,
    pub manual: bool,
    pub desktop: bool,
    since: Instant,
    timer_running: bool,
}
impl Default for PlayerIdle {
    fn default() -> Self {
        Self {
            hidden: false,
            manual: false,
            desktop: true,
            since: Instant::now(),
            timer_running: false,
        }
    }
}
impl Mio {
    pub(super) fn wake_player(&mut self, cx: &mut Context<Self>) {
        if !self.full_player_open {
            return;
        }
        // The original keeps controls visible in narrow windows, even after F1.
        if !self.player_idle.desktop {
            if self.player_idle.hidden {
                self.player_idle.hidden = false;
                cx.notify();
            }
            return;
        }
        if self.player_idle.manual {
            return;
        }
        self.player_idle.since = Instant::now();
        if self.player_idle.hidden {
            self.player_idle.hidden = false;
            cx.notify();
        }
        self.arm_player_idle(cx);
    }

    pub(super) fn arm_player_idle(&mut self, cx: &mut Context<Self>) {
        if self.player_idle.timer_running
            || !self.full_player_open
            || !self.auto_hide_bottom
            || !self.player_idle.desktop
            || self.player_idle.manual
            || self.player_settings_open
            || self.comments.open
        {
            return;
        }
        self.player_idle.timer_running = true;
        // One timer per entity. Mouse moves only extend its deadline.
        cx.spawn(async move |this, cx| {
            let mut wait = Duration::from_secs(3);
            loop {
                cx.background_executor().timer(wait).await;
                let Ok(next) = this.update(cx, |this, cx| {
                    if !this.full_player_open
                        || !this.auto_hide_bottom
                        || !this.player_idle.desktop
                        || this.player_idle.manual
                        || this.player_settings_open
                        || this.comments.open
                    {
                        this.player_idle.timer_running = false;
                        return None;
                    }
                    let elapsed = this.player_idle.since.elapsed();
                    if elapsed < Duration::from_secs(3) {
                        return Some(Duration::from_secs(3) - elapsed);
                    }
                    this.player_idle.timer_running = false;
                    this.player_idle.hidden = true;
                    cx.notify();
                    None
                }) else {
                    break;
                };
                match next {
                    Some(delay) => wait = delay,
                    None => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn set_auto_hide_bottom(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_hide_bottom = enabled;
        if !self.player_idle.manual {
            self.player_idle.hidden = false;
            self.player_idle.since = Instant::now();
            self.arm_player_idle(cx);
        }
        self.save_preferences();
        cx.notify();
    }
}
