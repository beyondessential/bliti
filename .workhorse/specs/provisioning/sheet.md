---
id: PSHT
---

# Label sheets

The [provisioning client](client.md) keeps a list of the labels of the boards it has provisioned, and makes a PDF sheet of labels for an operator to print on sticker paper.
It draws the labels from the results of the step `label` of [PLBL](label.md), and needs no device to do it.

## Borrowed terms

| term | meaning |
| --- | --- |
| payload | As [QR](../qr-code.md) has it. |
| suffix | As [PRV](overview.md#the-advertised-name) has it. It is the last four characters of a payload. |
| local storage | As [PWEB](client.md#tailscale-details) uses it. |

## The list

The provisioning client MUST add a label to its list when it sees a `provision-state` in which the step `label` is `passed` and its result carries `payload`, `version` and `at`, whether or not it watched the step pass.

A label MUST hold its payload, its version, the `at` of the result it came from, and when it was last printed, where it has been.

The list MUST hold at most one label for a payload.

A result for a payload the list already holds MUST replace that label, and the replacement MUST be unprinted.

For each suffix it has added a label for, the provisioning client MUST keep the `at` of the latest result it added, and MUST ignore a result whose `at` is not later.

The provisioning client MUST keep the `at` of a label it has removed for a week after the removal.

The provisioning client MUST NOT keep the payload of a label it has removed.

> [!NOTE]
> A device holds a finished run until it is powered off, so an operator who checks back at a finished board sees the same result again.
> Comparing `at` keeps that from adding a label that was already printed or removed, and running `label` again, which makes a later `at`, adds it again.

## Keeping the list

The provisioning client MUST keep the list in local storage, so that every tab and every visit of the browser holds the same one.

A label MUST stay in the list until the operator removes it.

The provisioning client MUST NOT lose a label that another tab added while it changes the list.

The provisioning client MUST show the list as it stands in storage each time it shows the list, and MUST keep it current while it is shown.

Where local storage cannot be used, the provisioning client MUST hold the list for as long as the page is open, and MUST say that the labels are lost when it is closed.

> [!NOTE]
> A bench of boards means several tabs, and each adds its boards' labels to the one list.
> The payload holds the presence token, so the browser of the bench holds the credential of every board it has not had removed, as [SEC](../security.md) records.

## Where the list is offered

The provisioning client MUST offer opening the list while finding a device and when the run has finished, whenever the list holds a label, and MUST say how many labels in it are unprinted.

Where the list holds an unprinted label, the provisioning client MUST offer downloading the sheet of the unprinted labels as the primary action of the finished screen.

## Choosing what to print

The list MUST show the latest label first, and each label with its suffix, its version, when it was added, and whether and when it was printed.

The list MUST tick each unprinted label, MUST leave each printed label unticked, and MUST let the operator tick and untick any label.

The list MUST offer downloading the sheet of the labels ticked as the primary action, and MUST NOT offer it while no label is ticked.

Wherever the provisioning client offers downloading a sheet, it MUST say how full the sheet is in place of how many labels it holds, as the table below has it.

| the sheet holds | the action says | example |
| --- | --- | --- |
| labels for part of one page | the percentage of the page's positions filled, to the nearest whole percent, then "filled" | 15% filled |
| labels for several pages, the last part filled | the number of pages, then the percentage the last holds | 3 sheets, last 70% filled |
| labels that fill every page | the number of pages | 2 sheets |

> [!NOTE]
> The action tells an operator how many sheets of sticker paper to load and how much of the last is wasted.

The provisioning client MUST mark each label on the sheet it downloaded as printed, as of the download.

The list MUST let the operator remove any label, and MUST ask the operator to confirm removing one that is unprinted.

> [!NOTE]
> The client cannot know a sheet was printed, so a download counts.
> A printed label that is wanted again is ticked and downloaded again, so a jam or a lost file costs nothing.

## The sheet

The sheet MUST be a PDF of A4 pages in portrait.

Each page MUST hold a grid of labels of 50 mm by 30 mm, three across and nine down, centred on the page.

The sheet MUST place the labels in the order of the list, filling each row from the left and each page from the top, and MUST add a page when one is full.

The sheet MUST draw a cut line around each label it holds, and MUST NOT draw one around a position that holds none.

A label MUST carry, from the left of its area:

| part | where | holds |
| --- | --- | --- |
| the QR code | on the left, as high as the label | the payload's text, as [QR](../qr-code.md#the-code) has it, at error correction level H, with its quiet zone |
| the suffix | on the right, large | the suffix |
| the product | beneath the suffix, small | "Tamanu Iti v" followed by the version |

A label MUST NOT carry the recovery passphrase, or anything else of the board's record.

The provisioning client MUST draw the sheet without a channel to a device, and MUST offer downloading it offline once it is ready offline, as [WEB](../web-app.md) requires of everything it needs.

> [!NOTE]
> The code fills the label's height, so a module is a little under 0.7 mm at the symbol version 5 that [QR](../qr-code.md#the-code) gives every device.
> A production label printer is chosen later, and the sheet is for stickers cut by hand from a laser printer's page.
