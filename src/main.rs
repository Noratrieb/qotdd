use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use eyre::{bail, ensure, Context, Result};
use rand::seq::SliceRandom;
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let port: u16 = match std::env::var("QUOTDD_PORT") {
        Ok(port) => match port.parse() {
            Ok(port) => port,
            Err(err) => {
                bail!("error: invalid port passed in QUOTDD_PORT: {err}");
            }
        },
        Err(_) => 17,
    };

    let quotes = [
        "Quickness is the essence of the war. ~ Sun Tsu",
        "Pretend inferiority and encourage his arrogance. ~ Sun Tsu",
        "meow. ~ wffl",
    ];

    ensure!(!quotes.is_empty(), "Quotes are empty");

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

    eprintln!("info: Listening on socket {}", addr);

    let listener = TcpListener::bind(addr)
        .await
        .wrap_err_with(|| format!("binding on port {port}"))?;

    let mut reset = tokio::time::interval(Duration::from_secs(60));

    let mut limits = RateLimits::default();

    loop {
        tokio::select! {
            result = tcp_loop(&listener, &quotes, &mut limits) => {
                result?;
            }
            _ = reset.tick() => {
                limits.lower();
            }
        }
    }
}

async fn tcp_loop(listener: &TcpListener, quotes: &[&str], limits: &mut RateLimits) -> Result<()> {
    let (mut conn, peer) = listener.accept().await.wrap_err("accepting connection")?;

    if !limits.accept(peer.ip()) {
        conn.shutdown().await.wrap_err("closing connection")?;
        return Ok(());
    }

    let quote = quotes.choose(&mut rand::thread_rng()).unwrap();

    conn.write_all(quote.as_bytes())
        .await
        .wrap_err("writing quote")?;
    conn.write_all(b"\n").await.wrap_err("writing quote")?;

    conn.shutdown().await.wrap_err("closing connection")?;
    Ok(())
}

// To avoid DoS amplification attacks, we ratelimit our service based on the IP.
// Sorry, but you're not getting many quotes...
#[derive(Default)]
struct RateLimits {
    ips: HashMap<IpAddr, usize>,
}

impl RateLimits {
    fn accept(&mut self, addr: IpAddr) -> bool {
        let count = self.ips.entry(addr).or_default();
        let old = *count;
        *count += 1;
        old < 10
    }

    fn lower(&mut self) {
        self.ips
            .values_mut()
            .for_each(|v| *v = v.saturating_sub(10));
        self.ips.retain(|_, v| *v > 0);
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::RateLimits;

    #[test]
    fn ratelimit() {
        let mut limits = RateLimits::default();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        for _ in 0..10 {
            assert!(limits.accept(ip))
        }

        for _ in 0..10 {
            assert!(!limits.accept(ip))
        }

        limits.lower();

        assert!(!limits.accept(ip));

        limits.lower();

        assert!(limits.accept(ip));
    }
}
