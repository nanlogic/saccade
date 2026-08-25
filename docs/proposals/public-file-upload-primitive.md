# Public bounded file-upload primitive

Status: owner-directed implementation proposal, 2026-08-24.

## Decision

Permit `saccade.act` to accept `operation: upload` only for a current
`file_input` object that advertises the `upload` affordance. The request must
carry one absolute path to an accessible, regular, non-symlink file. Runtime
uses the existing finite native file-chooser plan and verifies only the
value-free `has_value` transition. The local path never enters Truth or an
action receipt.

All other public controls remain object-addressed and software-first. This
exception adds no selectors, arbitrary coordinates, general native-input
surface, directory upload, or batch upload.

## Recognition

The Extension may recognize a generic custom upload/drop target when it owns
exactly one file input and has rendered semantic upload/drop evidence plus an
explicit pointer interaction signal. Generic pointer tabs may project as
`tab` only when their class/role context and rendered pointer behavior agree.

## Motivation

Real administration surfaces commonly hide the native file input behind a
custom drop target. Truth previously exposed neither the visible trigger nor a
public way to complete the bounded native chooser, leaving a known finite
control unusable even though the Reference Actuator already implemented and
tested the chooser primitive.
