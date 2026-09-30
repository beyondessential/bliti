# T2 test cases

## Low-battery shutdown

- [ ] A daemon on a machine with no backup board (a dev laptop running on its own battery, below 5 %) never begins a low-battery shutdown and logs nothing about being unable to power off (verifies spec: LOW)
- [x] A dev build deployed to a prototype Pi on an X120x board arms the shutdown as a release build does (verifies spec: LOW)
- [x] Sixty seconds of readings all below 2.8 V with external power absent powers the device off, and not a reading sooner (verifies spec: LOW)
- [x] A reading at or above 2.8 V, a reading that could not be taken, or external power returning starts the sixty seconds again (verifies spec: LOW)
- [x] Within two minutes of the system starting no shutdown begins, however long the floor has been held, and one begins once the two minutes are up (verifies spec: LOW)
- [x] Only a look where both the gauge and the power line answered counts towards the floor: no gauge, or a gauge with no line, never powers off (verifies spec: LOW)
- [x] A low-battery shutdown still under way is not begun again, and records the run once; after an act accepted first, no shutdown is begun as well (verifies spec: LOW, CTL)
- [x] A low-battery shutdown whose power-off failed is begun again once the floor has been held another sixty seconds (verifies spec: LOW, CTL)
- [x] A device that cannot power off says so once each time the floor is held for another sixty seconds, not at every reading (verifies spec: LOW)
- [x] What the run taught is recorded after the shutdown is certain and before any feed is told `going-away`, and not at all where the shutdown is not begun (verifies spec: CHG, LOW)
- [x] The report of powering off carries the cell voltage and how long external power has been absent, or no time away where it was absent from start (verifies spec: LOW)

## Charge and direction

- [x] Off mains, the charge is the discharging curve's at the cell voltage, 0 at or below the floor and 1 at full (verifies spec: CHG)
- [x] On mains, the charge is the gauge's figure scaled to its reading on a full cell, and unscaled until one is known (verifies spec: CHG)
- [x] On mains, a charging curve is read only once learnt from three charges and only within the voltages it covers (verifies spec: CHG)
- [x] A finished charge reports 1 and `idle` until external power next goes, and records the gauge's reading as its full one (verifies spec: CHG, NFO)
- [x] Constant-voltage charging, sitting at termination and still, is not taken for a finished charge (verifies spec: CHG)
- [x] A cell still falling after termination, or rising back to it, is not yet full (verifies spec: CHG)
- [ ] A real charge on each board is detected as finished within minutes of the charger stopping, and not before (verifies spec: CHG)
- [x] While the cell carries the device, `battery-charge` is `warning` below 0.2 and `failed` below 0.05, saying the battery is low, for the backup board's cell and an operating system's battery alike (verifies spec: NFO)
- [x] On mains, or fed around the backup board, a low charge is not reported as low; a disagreement between the power line and the cell's movement is reported instead where it holds (verifies spec: NFO)

## Supply state and curves

- [x] With no gauge, no curve file is read or written and a load or reset is refused as managing no backup supply (verifies spec: CRV)
- [x] The curve file is read the first time the gauge answers, and its document is the one in force (verifies spec: CHG, CRV)
- [x] A load or reset is saved before it takes effect and is sent to every curve subscriber; one that cannot be saved changes nothing (verifies spec: CRV)
- [x] A run is recorded from external power going, or from start on battery; a charge from external power returning, anchored where the run ended (verifies spec: CHG)
- [ ] The daemon's log on a prototype carries `charge` and `gauge_charge` on every backup-supply report (verifies spec: DEV)

## Learning

- [x] A run from full that reached the floor reshapes the whole discharging curve to the cell it ran on, keeps what lies below the floor, and sets the duration from how long it took (verifies spec: CHG)
- [x] A run from below full leaves the curve above where it began as it was, and refines below from the curve's charge there (verifies spec: CHG)
- [x] A replaced cell's curve takes over from the old one's within a few runs, however many runs the old one was learnt from (verifies spec: CHG)
- [x] The error is measured against the curve before the run refines it, and weighed in as the run is; a run the curve already follows lowers it (verifies spec: CHG)
- [x] A run that never reached the floor, or too short or covering too little of the curve, teaches nothing and writes nothing (verifies spec: CHG)
- [x] Whatever the run, the refined document is one CRV accepts (verifies spec: CRV)
- [x] A low-battery run is learnt, saved and sent to every curve subscriber before the shutdown goes on (verifies spec: CHG, CRV)
- [x] The first charge from a known start to finished creates the charging curve, learnt from 1 with the count-derived error; a later one refines it where they overlap (verifies spec: CHG)
- [ ] A real run-down on the prototype refines the discharging curve, and the log says so with `learnt_from` and `error` before the device powers off (verifies spec: CHG)

## Time left

- [x] `battery-time-to-empty` is skipped, saying so, until the charge has been watched falling for five minutes, then is the charge over the rate with a `margin` in seconds (verifies spec: CHG, NFO)
- [x] The margin combines the figure's error at the rate with how much the rate has varied, in quadrature (verifies spec: CHG)
- [x] On mains the time is to full, so time to empty ends; once the charge has finished, or fed around the backup board, neither is reported (verifies spec: NFO)
- [x] The rate starts again when external power comes or goes (verifies spec: CHG)
- [x] An operating system's battery reports its time to empty while discharging and to full while charging, as upower or sysfs gives it, with no margin, and `skipped` where it gives none (verifies spec: NFO)
- [ ] On the prototype, time left while running on battery falls in step with the clock, and its margin narrows as the run goes on (verifies spec: CHG, NFO)
