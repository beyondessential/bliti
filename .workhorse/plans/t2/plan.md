# T2: calibrated battery charge and low-battery shutdown

## Hardware data still needed

- **The shipped discharging curve** (CHG): a run on the production cell (Samsung INR21700-58E, CC5563F101, per F1), from full until the board gives out, logged every 10 s. The curve runs down to the board's own cutoff, so every floor at or above 2.8 V falls within it.
- **What a finished charge looks like** in the cell voltage on each board, for CHG's "full as the voltage shows". During constant-voltage charging the voltage already sits at termination with charge still going in, so "at termination and still" alone reports full early. What's needed is whatever follows termination (the voltage relaxing back, or the recharge threshold).
- **The gauge's reading on a full cell**, for scaling its figure on mains until three charges have been learnt.
- **An untouched run-down**, still owed from the T2 comments: the true resting voltage after cutoff, and whether the board restarts the Pi by itself. The restart when mains returns is the hardware's own behaviour and stays out of the specs.

## Design notes

- The shutdown watcher belongs with the supply record in `facts/power/record.rs`: it already runs on its own 10 s thread whether or not sampling is on (DEV), which is what LOW requires.
- Curves live in `/var/lib/bliti/`, beside `network.json`, written atomically. What a run taught must be written before `going-away` is sent (CHG).
- Learning without a current sensor: time at the device's own draw stands in for charge. A run from full rescales the whole curve by the run's duration. A partial run is anchored at the curve's charge where it began, and refines only below. Weighting is exponential over runs, so a replaced cell takes over within a few.
- Charging curve: anchored at the discharging curve's figure when mains returns, and ending at full. Used once `learnt-from` reaches 3, and only within the voltage range it covers.
- The reported charge is renormalised between the floor in force and full, so V2's raised floor needs no re-learning.
- Command line and daemon: a load or reset from the command line has to reach the running daemon (CRV). Either the daemon watches the curve file, or the command line talks to the daemon. Choose at tech design.
- `battery_direction` in `facts/power.rs` gives `charging` on mains whenever the gauge reads 99 % or less. Full cells seldom reach that, so a full device on mains reports `charging` forever. CHG's definition of full replaces the test.

## Rename: control stream to power stream

The CTL rename touches `bliti-core` (`channel/messages.rs`, `channel/generate.rs`), `bliti` (`control.rs`, `session.rs`, `client.rs`, session tests), and `bliti-web` (`lib.rs`, `app/src/Control.jsx`, `app/src/client.js`, `app/tests/control.spec.js`, `app/tests/fake-client.js`). The opening message becomes `power`, and `going-away` gains `cause`.
