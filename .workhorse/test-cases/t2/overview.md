# T2 test cases

## Low-battery shutdown

- [ ] A daemon on a machine with no backup board (a dev laptop running on its own battery, below 5 %) never begins a low-battery shutdown and logs nothing about being unable to power off (verifies spec: LOW)
- [ ] A dev build deployed to a prototype Pi on an X120x board arms the shutdown as a release build does (verifies spec: LOW)
