## Quint specification for proposal parts streaming

Install the pinned CLI with `make -C code install-quint` from the repository root. Run these commands from `specs/block-streaming`. Apalache requires Java.

The model delivers four parts in any order. CI checks the declared invariant at every reachable state through four deliveries, then searches for a state where FIN arrives before INIT and a state where the full stream has been emitted.

```sh
quint verify part_stream.qnt --backend=apalache --max-steps=4 --search-observations
```

The reachability report includes `finBeforeInit was observed` and `completeStream was observed`. To save a concrete delivery order that completes the stream, run:

```sh
quint run part_stream.qnt --observation=completeStream --max-steps=4 --out-itf=complete-stream.itf.json
```

The model also defines the temporal property `eventuallyDone`. The following older trace illustrates the loop that the temporal encoding adds after delivery.

```
$ quint verify part_stream.qnt --max-steps 5 --temporal 'not(eventually(isDone))'
State 5: state invariant 0 violated.                              I@16:15:04.901
Found 1 error(s)                                                  I@16:15:04.991
The outcome is: Error                                             I@16:15:05.644
An example execution:

[State 0]
{
  __InLoop: false,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: false,
  __temporal_t_3_unroll_prev: false,
  state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 }
}

[State 1]
{
  __InLoop: false,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: false,
  __temporal_t_3_unroll_prev: false,
  state:
    {
      buffer: [],
      emitted: [{ msgType: INIT, payload: "Init", sequence: 0 }],
      finReceived: false,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 1,
      received: Set({ msgType: INIT, payload: "Init", sequence: 0 }),
      totalMessages: 0
    }
}

[State 2]
{
  __InLoop: false,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: false,
  __temporal_t_3_unroll_prev: false,
  state:
    {
      buffer: [(2, { msgType: DATA, payload: "Data 2", sequence: 2 })],
      emitted: [{ msgType: INIT, payload: "Init", sequence: 0 }],
      finReceived: false,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 1,
      received: Set({ msgType: DATA, payload: "Data 2", sequence: 2 }, { msgType: INIT, payload: "Init", sequence: 0 }),
      totalMessages: 0
    }
}

[State 3]
{
  __InLoop: false,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: false,
  __temporal_t_3_unroll_prev: false,
  state:
    {
      buffer: [(2, { msgType: DATA, payload: "Data 2", sequence: 2 }), (3, { msgType: FIN, payload: "Fin", sequence: 3 })],
      emitted: [{ msgType: INIT, payload: "Init", sequence: 0 }],
      finReceived: true,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 1,
      received: Set({ msgType: DATA, payload: "Data 2", sequence: 2 }, { msgType: FIN, payload: "Fin", sequence: 3 }, { msgType: INIT, payload: "Init", sequence: 0 }),
      totalMessages: 4
    }
}

[State 4]
{
  __InLoop: false,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state: { buffer: [], emitted: [], finReceived: false, initMessage: None, initReceived: false, nextSequence: 0, received: Set(), totalMessages: 0 },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: true,
  __temporal_t_3_unroll_prev: false,
  state:
    {
      buffer: [],
      emitted:
        [
          { msgType: INIT, payload: "Init", sequence: 0 },
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 }
        ],
      finReceived: true,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 4,
      received:
        Set(
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 },
          { msgType: INIT, payload: "Init", sequence: 0 }
        ),
      totalMessages: 4
    }
}

[State 5]
{
  __InLoop: true,
  __q::temporalProps_init: false,
  __saved___temporal_t_1: false,
  __saved___temporal_t_2: false,
  __saved___temporal_t_3: true,
  __saved_state:
    {
      buffer: [],
      emitted:
        [
          { msgType: INIT, payload: "Init", sequence: 0 },
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 }
        ],
      finReceived: true,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 4,
      received:
        Set(
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 },
          { msgType: INIT, payload: "Init", sequence: 0 }
        ),
      totalMessages: 4
    },
  __temporal_t_1: false,
  __temporal_t_2: false,
  __temporal_t_3: true,
  __temporal_t_3_unroll: true,
  __temporal_t_3_unroll_prev: true,
  state:
    {
      buffer: [],
      emitted:
        [
          { msgType: INIT, payload: "Init", sequence: 0 },
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 }
        ],
      finReceived: true,
      initMessage: Some({ msgType: INIT, payload: "Init", sequence: 0 }),
      initReceived: true,
      nextSequence: 4,
      received:
        Set(
          { msgType: DATA, payload: "Data 1", sequence: 1 },
          { msgType: DATA, payload: "Data 2", sequence: 2 },
          { msgType: FIN, payload: "Fin", sequence: 3 },
          { msgType: INIT, payload: "Init", sequence: 0 }
        ),
      totalMessages: 4
    }
}

[violation] Found an issue (172272ms).
error: found a counterexample
```
