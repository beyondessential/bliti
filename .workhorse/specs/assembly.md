---
id: ASM
---

# Assembly guide

The assembly guide is the page of the [web application](web-app.md) that takes someone from the parts of an Iti to one that is built, provisioned and tested.

## The page

The assembly guide MUST be the page of [WEB](web-app.md) at `/assemble`.

The assembly guide MUST title itself Assemble an Iti, and MUST call the device an Iti.

The assembly guide MUST take the reader through the parts and tools, assembling the Iti with its case open, provisioning it, closing the case, and testing it, in that order.

The assembly guide MUST link to the files of each 3D-printed part.

The assembly guide MUST link to the page of [WEB](web-app.md) that reaches a device where it has the reader test the Iti.

## Provisioning

The assembly guide MUST link to the provisioning client of [PWEB](provisioning/client.md) where it has the reader provision the Iti.

The assembly guide MUST link to the provisioning image, its digest and its manifest at the paths [PMED](provisioning/medium.md#publishing) publishes them.

The assembly guide MUST state the size of USB stick the provisioning image fits, as [PMED](provisioning/medium.md#building) has it.

The assembly guide MUST tell the reader to download the provisioning image again, rather than reuse a medium written some time before.

> [!NOTE]
> The provisioning client refuses a medium built too long ago, as [PWEB](provisioning/client.md#refusing-a-medium) has it.
