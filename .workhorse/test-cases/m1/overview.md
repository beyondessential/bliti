# M1 — Battery from the OS test cases

Scenarios that verify a machine without the I2C gauge reports a battery. Spec: NFO (`device-info.md`), VIEW (`device-view.md`).

## Source selection

- [ ] With the I2C gauge absent and an OS battery present, the device reports `battery-charge`, `battery-voltage` and `battery-direction` (verifies spec: NFO)
- [ ] With the I2C gauge present, the OS path is not consulted and the reading is unchanged (verifies spec: NFO)
- [ ] With neither the I2C gauge nor any device battery, the battery readings are omitted entirely (verifies spec: NFO)

## Which battery counts

- [ ] A peripheral battery (mouse: type=Battery, scope=Device) is not reported (verifies spec: NFO)
- [ ] The machine's own battery (BAT0, no scope file) is reported (verifies spec: NFO)
- [ ] Where several batteries qualify, each is reported once and told apart by the `battery` trait (verifies spec: NFO)
- [ ] A machine with only peripheral batteries reports no device battery (verifies spec: NFO)

## Reading values

- [ ] `battery-charge` carries state of charge as a fraction (verifies spec: NFO)
- [ ] `battery-voltage` carries cell voltage in volts, rounded (verifies spec: NFO)
- [ ] A battery reporting no voltage yields `battery-voltage` as `skipped` with a reason (verifies spec: NFO)
- [ ] `battery-direction` is `charging` / `discharging` / `idle` from the OS charging state (verifies spec: NFO)
- [ ] The OS reporting an unknown charging state yields `battery-direction` as `skipped` (verifies spec: NFO)
- [ ] The `battery` trait carries name, and serial/model/vendor where held (verifies spec: NFO)

## Interaction with power-source

- [ ] An OS-read battery reports no `power-source` (verifies spec: NFO)
- [ ] `battery-direction` on the OS path is reported immediately, not gated on a voltage-watch window (verifies spec: NFO)

## View

- [ ] The view headlines `battery-charge` with the first battery and shows every battery in the reveal (verifies spec: VIEW)
- [ ] Each battery's voltage and direction appear in that battery's reveal, paired by the `battery` trait (verifies spec: VIEW)

## On this hardware

- [ ] Run on the dev laptop: BAT0 reported with a plausible charge, voltage and direction; the wireless mouse's battery is absent
