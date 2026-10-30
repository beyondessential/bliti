---
id: PCON
---

# Provisioning console command

An operator drives a [run](../overview.md#run) from the console of the provisioning system, in place of a client, through a command the system provides.

The console is the system's own text console, whether a keyboard and display or a serial line.

## The command

A device in provisioning mode MUST provide a command, run on its console, that offers every act of [PRS](stream.md) and shows every part of the state a client shows.

The command MUST list the steps in order, each with its done-state and reason, and MUST let the operator choose the steps to run.

The command SHOULD offer a step reported `done` unticked, as [RUN](run.md) says of a client.

The command MUST start a run of the steps chosen, and the steps MUST run in the same order and with the same effect as for a client.

While a run is under way, the command MUST show each step's state, the progress of a running step, why a step failed, and every open prompt as text.

The command MUST show the result of every step that reports one.

The command MUST take the answer to a prompt as text.

The command MUST show the default of a prompt that carries one, and MUST take an empty answer to it as the default.

The command MUST NOT display the characters of an answer to a `secret` prompt as they are typed.

The command MUST offer a retry and a stop where [RUN](run.md) accepts them, and MUST offer to acknowledge a finished run once it has shown every result.

## Independence

The command MUST work without the USER button having been pressed and without any client connected.

The command MUST work while a client is connected, and the two MUST show the same run.

A run MUST continue when the command exits or loses its console, as [RUN](run.md) requires of any front end.

> [!NOTE]
> The text of a prompt names the parts of the board by their silkscreen labels, as [RUN](run.md) requires, so the command needs no picture.
> It is the way to provision a board whose USER button does not work.
