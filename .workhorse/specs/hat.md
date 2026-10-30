---
id: HAT
---

# Iti HAT EEPROM

A Tamanu Iti of the fourth generation carries a HAT EEPROM, and that EEPROM is what tells software the board is a Tamanu Iti and which version of it.

This spec defines what the EEPROM holds, how a reader recognises a Tamanu Iti and its version from it, and the device tree overlay it names.
How provisioning writes it is specified under [PEEP](provisioning/eeprom.md), and how it installs the overlay into the board's system under [PDSK](provisioning/disk.md).
How the `product` fact reports what it records is specified under [NFO](device-info.md#product).

## Borrowed terms

| term | meaning |
| --- | --- |
| HAT+ | The Raspberry Pi specification of a HAT EEPROM's contents, for boards fitted to the Raspberry Pi 5's header. |
| HAT EEPROM | As [NFO](device-info.md) has it. |
| product UUID | The 16-byte identifier HAT+ requires of a HAT's vendor information, unique to a product and the same on every board of it. |
| device tree overlay | A compiled description of hardware that the Pi's firmware merges into the device tree it hands the operating system at boot. |
| TPM | A Trusted Platform Module, as [BID](board-id.md) has it. |

## Format

A Tamanu Iti's HAT EEPROM MUST conform to HAT+, in header format version `0x02`.

It MUST hold a vendor-information atom and a device tree overlay name atom.

## Contents

The vendor-information atom MUST hold:

| field | value |
| --- | --- |
| `uuid` | `7c3b8a88-ad89-4d48-b709-2bd445729867`, the product UUID of the Tamanu Iti |
| `product_id` | the major version, which is 4 for the fourth generation |
| `product_ver` | the minor version, which is 0 for revision B and 1 for revision C |
| `vendor` | "BES International" |
| `product` | "Tamanu Iti v" followed by the major and minor versions joined by a full stop, as "Tamanu Iti v4.0" is |

The device tree overlay name atom MUST name `tamanu-iti-v4`.

A Tamanu Iti's version is its major and minor versions, joined by a full stop: revision B is 4.0 and revision C is 4.1.

> [!NOTE]
> HAT+ makes the product UUID per product and has no field for a serial, so nothing in the EEPROM tells one board from another.
> Each board is told apart by its board ID, which [BID](board-id.md) derives from its TPM.
> HAT+ leaves the meaning of `product_id` and `product_ver` to the vendor, and there is no registry of vendors, so each means something only beside the product UUID.

## Recognising a Tamanu Iti

The Pi's firmware publishes a programmed HAT EEPROM as text files under `/proc/device-tree/hat/`, one per field: `uuid`, `product_id`, `product_ver`, `vendor` and `product`.
It publishes `uuid` hyphenated, `product_id` and `product_ver` as `0x` followed by hexadecimal digits, as `0x0004`, and ends each file's text with a NUL.
It publishes nothing under that path where no EEPROM is fitted or the EEPROM is blank.

A reader MUST take a board for a Tamanu Iti where, and only where, `/proc/device-tree/hat/uuid` holds the product UUID of [Contents](#contents).

A reader MUST take the board's version from `/proc/device-tree/hat/product_id` and `/proc/device-tree/hat/product_ver`, and MUST NOT read either from a board whose `uuid` is another.

A reader MUST NOT take a board for a Tamanu Iti where either of those files is absent, or holds text other than `0x` followed by hexadecimal digits.

A reader MUST NOT use `vendor` or `product` to recognise a Tamanu Iti or to find its version.

> [!NOTE]
> The strings are for whoever reads the board, and a reader that matched on them would tie recognition to a spelling.
> A board whose `uuid` is the Tamanu Iti's and whose version cannot be read is not recognised, so [NFO](device-info.md#product) reports it as it does any board that carries a TPM and no identifying EEPROM.

## Device tree overlay

The overlay `tamanu-iti-v4` MUST enable the first SPI bus, and MUST declare the board's TPM on that bus's second chip select as a generic TIS-over-SPI device.

> [!NOTE]
> The generic binding describes the ST33KTPM accurately and is one the kernel's TPM driver accepts, so the overlay need not name a manufacturer's.

The overlay MUST enable the third I²C bus on GPIO22 and GPIO23, at 100 kHz.

The overlay MUST NOT bind a kernel driver to any device on that bus.

> [!NOTE]
> A bound driver holds its device's address, and would stop software that reads the device directly from reaching it.
> The overlay grows when a feature needs a kernel driver or an input device, and the feature's spec says so.

The overlay MUST be among the overlays of the boot partition of any system that runs on a board whose HAT EEPROM names it.

> [!NOTE]
> The firmware loads the overlay from the boot partition, and a board whose EEPROM names an overlay the partition lacks has none of its devices configured.
