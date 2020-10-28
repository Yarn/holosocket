
# Event Types

A `Live` is a json object matching the structure of one object within the
live array in the response of https://api.holotools.app/api-docs#tag/Live
with `?hide_channel_desc=1`.

## `initial`

This event is sent once on initial connection 

Data is the same format as https://api.holotools.app/api-docs#tag/Live
with `?max_upcoming_hours=2190&hide_channel_desc=1` without the `cached`
field.

This json object is the initial `state`.

---
## `live_add`

data type: `Live`

Add a `Live` to `state.live`.

---
## `live_rem`

data type: `number`

A `Live` removed from `state.live`.

---
## `upcoming_add`

data type: `Live`

Add a `Live` to `state.upcoming`.

---
## `upcoming_rem`

data type: `number`

A `Live` removed from `state.upcoming`.

---
## `ended_add`

data type: `Live`

Add a `Live` to `state.ended`.

---
## `ended_rem`

data type: `number`

A `Live` removed from `state.ended`.
