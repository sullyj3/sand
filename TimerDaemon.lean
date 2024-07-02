import Socket

open System (FilePath)
open Socket (SockAddr)

/- def incCounter : IO.AtomicT Nat m Nat := do -/

def handleClient 
  (client : Socket)
  (_clientAddr : SockAddr)
  (counter : IO.Mutex Nat)
  : IO Unit := do
  let n ← counter.atomically do
    let n ← get
    set <| n + 1
    return n
  let bytes ← client.recv (maxBytes := 1024)
  let msg := String.fromUTF8! bytes
  IO.println s!"received message from client #{n}: {msg}"
    

def main : IO Unit := do

  let sockPath : FilePath := "/home/james/tmp/lean-timer-socket"
  if (← sockPath.pathExists) then do
    IO.FS.removeFile sockPath

  let addr := Socket.SockAddrUnix.unix sockPath
  let sock : Socket ← Socket.mk .unix .stream

  sock.bind addr
  sock.listen 5

  IO.println "listening..."

  let counter ← IO.Mutex.new 1

  while true do
    let (client, clientAddr) ← sock.accept
    let _tsk ← IO.asTask <|
      handleClient client clientAddr counter
