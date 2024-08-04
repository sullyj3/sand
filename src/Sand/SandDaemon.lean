import «Sand».Basic
import «Sand».Time
import «Sand».Message
import «Sand».Timers
import «Sand».SandDaemon.Basic
import «Sand».SandDaemon.HandleCommand

open System (FilePath)

open Sand

private def xdgDataHome : OptionT BaseIO FilePath :=
  xdgDataHomeEnv <|> dataHomeDefault
  where
    xdgDataHomeEnv  := FilePath.mk <$> (OptionT.mk <| IO.getEnv "XDG_DATA_HOME")
    home            := FilePath.mk <$> (OptionT.mk <| IO.getEnv "HOME"         )
    dataHomeDefault := home <&> (· / ".local/share")

def dataDir : OptionT BaseIO FilePath := xdgDataHome <&> (· / "sand")

def xdgSoundLocation : OptionT BaseIO FilePath := do
  let dir ← dataDir
  let soundPath := dir / "timer_sound.opus"
  guard (← soundPath.pathExists)
  pure soundPath

def usrshareSoundLocation : OptionT BaseIO FilePath := do
  let path : FilePath := "/usr/share/sand/timer_sound.opus"
  guard (← path.pathExists)
  pure path

partial def forever (act : IO α) : IO β := act *> forever act

def envFd : IO (Option UInt32) := OptionT.run do
  let str ← OptionT.mk <| IO.getEnv "SAND_SOCKFD"
  let some n := str.toNat?
    | throwThe IO.Error <|
      IO.userError "Error: Found SAND_SOCKFD but couldn't parse it as a string"
  return n.toUInt32

def systemdSockFd : UInt32 := 3

def SandDaemon.main (_args : List String) : IO α := do
  IO.eprintln s!"Starting Sand daemon {Sand.version}"

  let fd ← match ← envFd with
  | none => do
    IO.eprintln "SAND_SOCKFD not found, falling back on default."
    pure systemdSockFd
  | some fd => do
    IO.eprintln "found SAND_SOCKFD."
    pure fd
  let sock ← try (Socket.fromFd fd) catch err => do
    IO.eprintln s!"Error creating socket from file descriptor {fd}."
    throw err

  let soundPath? ← liftM (xdgSoundLocation <|> usrshareSoundLocation).run
  if let none := soundPath? then
    IO.eprintln "Warning: failed to locate notification sound. Audio will not work"

  let state ← DaemonState.initial
  IO.eprintln s!"Sand daemon started. listening on fd {fd}"
  forever do
    let (client, _clientAddr) ← sock.accept
    let _tsk ← IO.asTask (prio := .dedicated) <| do
      let clientConnectedTime ← Moment.mk <$> IO.monoMsNow
      let env := {state, client, clientConnectedTime, soundPath?}
      handleClient env
