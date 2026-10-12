//! Exclusive input authority. Observations do not acquire or extend this lease.
//! The owned launcher generates tokens from its OS randomness source; tokens
//! are never accepted from the player as an acquisition override.
use crate::{
    actions::Epoch,
    error::{Error, Result},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

struct Lease {
    token: String,
    expires: Instant,
}
pub struct InputAuthority {
    holder: String,
    epoch: Epoch,
    lease: Option<Lease>,
    issued: BTreeSet<String>,
}
impl InputAuthority {
    pub fn new(holder: String, epoch: Epoch) -> Result<Self> {
        if holder.is_empty()
            || holder.len() > 128
            || !holder
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
        {
            return Err(Error::new(
                "invalid_controller",
                "Configured controller identifier required",
            ));
        }
        Ok(Self {
            holder,
            epoch,
            lease: None,
            issued: BTreeSet::new(),
        })
    }
    fn duration(seconds: u64) -> Result<Duration> {
        if !(30..=900).contains(&seconds) {
            return Err(Error::new(
                "invalid_lease",
                "Lease duration must be 30..900 seconds",
            ));
        }
        Ok(Duration::from_secs(seconds))
    }
    pub fn acquire(
        &mut self,
        holder: &str,
        token: String,
        seconds: u64,
        epoch: &Epoch,
    ) -> Result<Instant> {
        self.acquire_at(holder, token, seconds, epoch, Instant::now())
    }
    fn acquire_at(
        &mut self,
        holder: &str,
        token: String,
        seconds: u64,
        epoch: &Epoch,
        now: Instant,
    ) -> Result<Instant> {
        let duration = Self::duration(seconds)?;
        if holder != self.holder || epoch != &self.epoch {
            return Err(Error::new(
                "ownership_changed",
                "Controller or campaign binding differs",
            ));
        }
        if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::new(
                "invalid_lease",
                "Launcher-generated token required",
            ));
        }
        if self.lease.as_ref().is_some_and(|v| now < v.expires) {
            return Err(Error::new(
                "input_busy",
                "Existing controller must renew or release its lease",
            ));
        }
        // A released/expired token is never a valid acquisition token again.
        if self.issued.contains(&token) {
            return Err(Error::new(
                "invalid_lease",
                "Fresh acquisition token required",
            ));
        }
        if self.issued.len() >= 4096 {
            return Err(Error::new(
                "lease_capacity",
                "Session lease-generation capacity reached",
            ));
        }
        let expires = now + duration;
        self.issued.insert(token.clone());
        self.lease = Some(Lease { token, expires });
        Ok(expires)
    }
    pub fn authorize(&self, token: &str, epoch: &Epoch) -> Result<Instant> {
        self.authorize_at(token, epoch, Instant::now())
    }
    fn authorize_at(&self, token: &str, epoch: &Epoch, now: Instant) -> Result<Instant> {
        let lease = self
            .lease
            .as_ref()
            .filter(|v| v.token == token && now < v.expires);
        if epoch != &self.epoch || lease.is_none() {
            return Err(Error::new(
                "input_lease_required",
                "Fresh matching exclusive input lease required",
            ));
        }
        Ok(lease.unwrap().expires)
    }
    pub fn renew(&mut self, token: &str, seconds: u64, epoch: &Epoch) -> Result<Instant> {
        let duration = Self::duration(seconds)?;
        let now = Instant::now();
        self.authorize_at(token, epoch, now)?;
        let expires = now + duration;
        self.lease.as_mut().unwrap().expires = expires;
        Ok(expires)
    }
    pub fn release(&mut self, token: &str, epoch: &Epoch) -> Result<()> {
        self.authorize(token, epoch)?;
        // Keep the token tombstone so a delayed caller cannot reacquire it.
        self.lease.as_mut().unwrap().expires = Instant::now();
        Ok(())
    }
    pub fn status(&self) -> (&str, Duration) {
        (
            &self.holder,
            self.lease.as_ref().map_or(Duration::ZERO, |v| {
                v.expires.saturating_duration_since(Instant::now())
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn epoch() -> Epoch {
        Epoch {
            game: crate::platform::Identity {
                pid: 123,
                start_ticks: "5".into(),
                exe: "/fixture".into(),
                uid: 1000,
            },
            campaign: "fixture-generation".into(),
            player: "GER".into(),
        }
    }
    #[test]
    fn competing_acquisition_and_stale_epoch_cannot_replace_authority() {
        let epoch = epoch();
        let mut authority = InputAuthority::new("luna-player".into(), epoch.clone()).unwrap();
        let token = "a".repeat(32);
        authority
            .acquire("luna-player", token.clone(), 300, &epoch)
            .unwrap();
        assert_eq!(
            authority
                .acquire("luna-player", "b".repeat(32), 300, &epoch)
                .unwrap_err()
                .code,
            "input_busy"
        );
        let mut stale = epoch.clone();
        stale.game.start_ticks = "6".into();
        assert!(authority.authorize(&token, &stale).is_err());
        assert!(authority.authorize(&"b".repeat(32), &epoch).is_err());
        authority.authorize(&token, &epoch).unwrap();
        assert!(authority
            .acquire("builder", "c".repeat(32), 300, &epoch)
            .is_err());
        authority.authorize(&token, &epoch).unwrap();
    }
    #[test]
    fn expired_or_released_authority_does_not_revive_from_delayed_requests() {
        let epoch = epoch();
        let mut authority = InputAuthority::new("luna-player".into(), epoch.clone()).unwrap();
        let before = Instant::now() - Duration::from_secs(31);
        let token = "a".repeat(32);
        authority
            .acquire_at("luna-player", token.clone(), 30, &epoch, before)
            .unwrap();
        assert!(authority.authorize(&token, &epoch).is_err());
        assert!(authority.renew(&token, 300, &epoch).is_err());
        assert!(authority
            .acquire("luna-player", token.clone(), 300, &epoch)
            .is_err());
        let next = "b".repeat(32);
        authority
            .acquire("luna-player", next.clone(), 30, &epoch)
            .unwrap();
        authority.renew(&next, 300, &epoch).unwrap();
        authority.release(&next, &epoch).unwrap();
        assert!(authority.authorize(&next, &epoch).is_err());
        assert!(authority.acquire("luna-player", next, 300, &epoch).is_err());
        assert!(authority
            .acquire("luna-player", token, 300, &epoch)
            .is_err());
    }
}
