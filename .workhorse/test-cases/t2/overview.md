# T2 test cases

## Low-battery shutdown

- [ ] A daemon on a machine with no backup board (a dev laptop running on its own battery, below 5 %) never begins a low-battery shutdown and logs nothing about being unable to power off (verifies spec: LOW)
- [ ] A dev build deployed to a prototype Pi on an X120x board arms the shutdown as a release build does (verifies spec: LOW)
- [x] Sixty seconds of readings all below 2.8 V with external power absent powers the device off, and not a reading sooner (verifies spec: LOW)
- [x] A reading at or above 2.8 V, a reading that could not be taken, or external power returning starts the sixty seconds again (verifies spec: LOW)
- [x] Within two minutes of the system starting no shutdown begins, however long the floor has been held, and one begins once the two minutes are up (verifies spec: LOW)
- [x] Only a look where both the gauge and the power line answered counts towards the floor: no gauge, or a gauge with no line, never powers off (verifies spec: LOW)
- [x] Once a low-battery shutdown has begun, nothing more is tried; after an act accepted first, no shutdown is begun as well (verifies spec: LOW, CTL)
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
