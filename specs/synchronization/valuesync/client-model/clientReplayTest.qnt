module clientReplayTest {
  import valuesync(PEERS = Set("p1", "p2"), BATCH_SIZE = 2, MAX_PARALLEL = 2,
                   TOP = 4, HISTORY_MIN = 1).* from "./valuesync"

  type Input = Initial | Internal | Started | StatusUpdate((str, int))
             | Response((int, str, int, int)) | Decided(int) | Transient(int)
             | Disconnected(str)
  var boundary: Input

  // Rust handles a whole request pass on one external input. The Quint client
  // allocates one range per action. Compare once that same pass has completed.
  action inside(a) = all { a, boundary' = Internal }
  action observe(input) = all { client' = client, oracle' = oracle, boundary' = input }
  action begin = all { init, boundary' = Initial }
    .then(inside(startConsensus)).then(observe(Started))
  action status(p, h, requests) = h.reps(_ => inside(peerAdvance(p)))
    .then(if (requests == 0) inside(receiveStatus(p))
          else inside(receiveStatus(p)).then(requests.reps(_ => inside(requestValues(p)))))
    .then(observe(StatusUpdate((p, h))))

  action complete(id, p, lo, count) = inside(deliverFull(id))
    .then(observe(Response((id, p, lo, count))))
  action nextHeight(h) = inside(decide).then(observe(Decided(h)))

  run parallelCatchupTest = begin.then(status("p1", 4, 2))
    .then(complete(1, "p1", 3, 2)).then(complete(0, "p1", 1, 2))
    .then(nextHeight(1)).then(nextHeight(2)).then(nextHeight(3)).then(nextHeight(4))
    .expect(client.tip == 4 and client.pending == Map())

  run partialSuffixTest = begin.then(status("p1", 4, 2))
    .then(inside(deliverPartial(0, 1))).then(inside(requestValues("p1")))
    .then(observe(Response((0, "p1", 1, 1))))
    .expect(client.pending.get(2).range == {lo: 2, hi: 2})

  run disconnectReroutesPrefixTest = begin.then(status("p1", 2, 1)).then(status("p2", 1, 0))
    .then(inside(peerDisconnect("p1"))).then(observe(Disconnected("p1")))
    .expect(client.pending.get(1).range == {lo: 1, hi: 1})

  run retryDecidedPrefixTest = begin.then(status("p1", 2, 1)).then(complete(0, "p1", 1, 2))
    .then(nextHeight(1)).then(inside(retryServed(0, "p1", false))).then(observe(Transient(2)))
    .expect(client.tip == 1)
}
