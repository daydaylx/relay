{
  lib,
  runCommand,
  testers,
  writeShellScriptBin,
  writeShellScript,
  nix,
  hello,
  cowsay,
  figlet,
  coreutils,
  bash,
  relay,
  agent,
  nodejs_22,
}:

# Real activation and recovery, inside a NixOS VM (never on the host).
#
# What is real: sudo escalation, `nix-env` on the system profile, `switch-to-configuration`
# (dry-activate, test, switch, boot) including NixOS' own switch-inhibitor check, systemd and the
# health queries, the journal, source write/restore, undo and recovery.
#
# What is stubbed: only the *evaluation and build* of the candidate flake. The VM has no network
# and no way to build a NixOS system from source, so `nix eval`/`nix build` of the candidate are
# answered by a shim that returns pre-built candidate systems (NixOS specialisations of the VM's
# own configuration, selected by the content of the candidate's relay/managed.nix). Real
# evaluation and building are exercised separately against real Nix by Relay's own checks.

let
  render =
    name: args:
    runCommand "managed-${name}" { } ''
      ${relay}/bin/relay render-managed ${args} > $out
    '';

  managedBase = runCommand "managed-base" { } ''
    mkdir flake
    ${relay}/bin/relay init --flake "$PWD/flake" > /dev/null
    cp flake/relay/managed.nix $out
  '';
  managedGood = render "good" "add-package hello";
  managedBroken = render "broken" "add-package cowsay";
  managedOption = render "option" "set-option services.relay-demo.mode string 'fast mode'";
  managedFailing = render "failing" "set-option services.relay-test.enable bool true";
  managedInhibited = render "inhibited" "add-package figlet";

  # A stand-in "model" for the optional natural-language front end: it answers every prompt with
  # the same typed proposal. Relay validates it exactly like any other intent.
  aiProvider = writeShellScript "relay-test-provider" ''
    cat > /dev/null
    printf '%s' '{"schema":1,"changes":[{"op":"add_package","package":"hello"}]}'
  '';

  # Stands in for `nix` on Relay's PATH. Everything except evaluation/build is the real nix.
  nixShim = writeShellScriptBin "nix" ''
    set -eu
    verb=""
    for arg in "$@"; do
      case "$arg" in
        eval|build|store|--version) verb="$arg"; break ;;
      esac
    done
    case "$verb" in
      --version|store) exec ${nix}/bin/nix "$@" ;;
      eval)
        ref=""
        for arg in "$@"; do
          case "$arg" in *"#nixosConfigurations."*) ref="$arg" ;; esac
        done
        dir="''${ref%%#*}"
        dir="''${dir#path:}"
        case "$ref" in
          *.config.environment.etc) echo true ;;
          *.outPath)
            # What the live source would build to: the system its managed module belongs to.
            sum=$(sha256sum "$dir/relay/managed.nix" | cut -c1-32)
            out=$(grep "^$sum " /tmp/relay-candidates | cut -d' ' -f2) || true
            echo "''${out:-/nix/store/$sum-unknown}" ;;
          *.drvPath)
            sum=$(sha256sum "$dir/relay/managed.nix" | cut -c1-32)
            echo "/nix/store/$sum-nixos-system-machine.drv" ;;
          *) echo "unexpected reference $ref" >&2; exit 98 ;;
        esac ;;
      build)
        last=""
        for arg in "$@"; do last="$arg"; done
        drv="''${last%^out}"
        name="''${drv#/nix/store/}"
        sum="''${name%%-*}"
        out=$(grep "^$sum " /tmp/relay-candidates | cut -d' ' -f2) || true
        [ -n "$out" ] || { echo "no candidate for $sum" >&2; exit 97; }
        echo "$out" ;;
      *) echo "unexpected nix invocation: $*" >&2; exit 99 ;;
    esac
  '';
in
testers.runNixOSTest {
  name = "relay-activation";

  nodes.machine =
    { pkgs, ... }:
    {
      users.users.alice = {
        isNormalUser = true;
        extraGroups = [ "wheel" ];
      };
      # Relay runs as a normal user and escalates exactly the typed activation commands.
      security.sudo.wheelNeedsPassword = false;
      nix.settings.experimental-features = [
        "nix-command"
        "flakes"
      ];
      environment.systemPackages = [
        relay
        pkgs.jq
      ];

      # The test VM boots directly into its kernel; a real bootloader installation is not possible
      # (and not what is under test) in this VM.
      boot.loader.grub.enable = lib.mkForce false;

      # The running system publishes the managed module it was built from.
      environment.etc."relay/managed.nix".source = managedBase;
      system.switch.inhibitors.relay-test = "1";

      specialisation = {
        good.configuration = {
          environment.etc."relay/managed.nix".source = lib.mkForce managedGood;
          environment.systemPackages = [ hello ];
          systemd.services.relay-demo = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              ExecStart = "${coreutils}/bin/true";
            };
          };
        };
        # An option change (a string value) with a unit that becomes active.
        option.configuration = {
          environment.etc."relay/managed.nix".source = lib.mkForce managedOption;
          environment.etc."relay-option-demo".text = "fast mode";
          systemd.services.relay-option = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
              ExecStart = "${coreutils}/bin/true";
            };
          };
        };
        # Activation itself succeeds, but the new unit crashes some seconds later: only the health
        # check (with its observation window) can notice.
        broken.configuration = {
          environment.etc."relay/managed.nix".source = lib.mkForce managedBroken;
          environment.systemPackages = [ cowsay ];
          systemd.services.relay-broken = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              Type = "simple";
              ExecStart = "${bash}/bin/sh -c 'sleep 10; exit 1'";
              Restart = "no";
            };
          };
        };
        # The new unit fails while NixOS itself is activating (its start job fails).
        failing.configuration = {
          environment.etc."relay/managed.nix".source = lib.mkForce managedFailing;
          systemd.services.relay-failing = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              Type = "oneshot";
              ExecStart = "${coreutils}/bin/false";
            };
          };
        };
        # A switch inhibitor changes: NixOS itself refuses `test` and `switch`.
        inhibited.configuration = {
          environment.etc."relay/managed.nix".source = lib.mkForce managedInhibited;
          environment.systemPackages = [ figlet ];
          system.switch.inhibitors.relay-test = lib.mkForce "2";
        };
      };
    };

  testScript = ''
    import json
    import shlex

    start_all()
    machine.wait_for_unit("multi-user.target")

    ALICE = "sudo -u alice -H env PATH=${nixShim}/bin:$PATH sh -c "

    def relay(arguments, env=""):
        command = f"{env} relay {arguments} >/tmp/relay.out 2>/tmp/relay.err"
        status, _ = machine.execute(ALICE + shlex.quote(command))
        out = machine.succeed("cat /tmp/relay.out")
        err = machine.succeed("cat /tmp/relay.err")
        return status, out, err

    def relay_json(arguments, env="", expect=0):
        status, out, err = relay(arguments, env)
        if status != expect:
            print(machine.execute("tail -n +1 /home/alice/state/changes/*/*.log 2>&1")[1])
        assert status == expect, f"relay {arguments}: exit {status}, expected {expect}\nstdout: {out}\nstderr: {err}"
        return json.loads(out)

    def current():
        return machine.succeed("readlink -f /run/current-system").strip()

    def profile():
        return machine.succeed("readlink -f /nix/var/nix/profiles/system").strip()

    def managed():
        return machine.succeed("cat /home/alice/config/relay/managed.nix")

    # An installed system has a system profile; the test VM does not create one on its own.
    base = current()
    machine.succeed(f"nix-env -p /nix/var/nix/profiles/system --set {base}")
    assert profile() == base

    def specialisation(name):
        return machine.succeed(f"readlink -f {base}/specialisation/{name}").strip()

    def digest(path):
        return machine.succeed(f"sha256sum {path} | cut -c1-32").strip()

    good = specialisation("good")
    option = specialisation("option")
    broken = specialisation("broken")
    failing = specialisation("failing")
    inhibited = specialisation("inhibited")
    machine.succeed(
        "printf '%s %s\\n' "
        + f"{digest('${managedBase}')} {base} "
        + f"{digest('${managedGood}')} {good} "
        + f"{digest('${managedOption}')} {option} "
        + f"{digest('${managedBroken}')} {broken} "
        + f"{digest('${managedFailing}')} {failing} "
        + f"{digest('${managedInhibited}')} {inhibited} "
        + "> /tmp/relay-candidates"
    )

    with subtest("setup: the one-time init creates only the managed module"):
        machine.succeed("sudo -u alice mkdir -p /home/alice/config")
        machine.succeed("sudo -u alice sh -c 'printf \"{ outputs = _: {}; }\\n\" > /home/alice/config/flake.nix'")
        init = relay_json("init --flake /home/alice/config")
        assert init["created"] is True
        assert managed() == open("${managedBase}").read()
        status = relay_json("status --flake /home/alice/config")
        assert status["managed_module"] == "in-sync", status
        assert status["unresolved_change"] is None
        assert status["running_system_path"] == base
        health = relay_json("health")
        assert health["system_state"] in ("running", "degraded"), health

        with subtest("agent binary starts without Pi profile and reports read-only tools"):
            agent_check = machine.succeed("sudo -u alice -H ${agent}/bin/relay-agent --check")
            agent_data = json.loads(agent_check)
            assert agent_data["piConfigLoaded"] is False, agent_data
            assert agent_data["modelCanApplyWithoutLocalConfirmation"] is False, agent_data
            assert "relay_diagnose" in agent_data["tools"], agent_data
            machine.fail("test -e /home/alice/.pi")

        with subtest("the agent confirmation gate completes plan, apply, undo and recovery in the VM"):
            workflow = "${nodejs_22}/bin/node ${agent}/lib/relay-agent/node_modules/tsx/dist/cli.mjs --test ${agent}/lib/relay-agent/src/workflow.integration.test.ts"
            env = " ".join([
                "RELAY_AGENT_WORKFLOW_TEST=1",
                "RELAY_AGENT_FLAKE=/home/alice/config",
                "RELAY_AGENT_HOST=machine",
                "RELAY_AGENT_STATE_DIR=/home/alice/state/agent-workflow",
                "RELAY_CORE_PATH=${relay}/bin/relay",
            ])
            command = f"{env} PATH=${nixShim}/bin:$PATH {workflow}"
            status, output = machine.execute(ALICE + shlex.quote(command))
            assert status == 0, output
            assert "# pass 1" in output, output
            assert current() == base and profile() == base

    with subtest("plan leaves the live system untouched"):
        plan = relay_json("plan --flake /home/alice/config --host machine add-package hello --state-dir /home/alice/state")
        assert plan["risk"] == "LIVE_SWITCHABLE" and plan["applicable"], plan
        assert plan["candidate_system"] == good
        assert current() == base and profile() == base
        assert managed() == open("${managedBase}").read()
        machine.fail("test -e /run/current-system/sw/bin/hello")

    with subtest("apply: dry-activate, test, health, switch"):
        report = relay_json(f"apply {plan['id']} --yes --expect-active relay-demo.service --observe 1 --state-dir /home/alice/state")
        assert report["outcome"] == "switched", report
        assert current() == good and profile() == good
        machine.succeed("test -e /run/current-system/sw/bin/hello")
        machine.succeed("systemctl is-active relay-demo.service")
        assert managed() == open("${managedGood}").read()
        # The runtime publishes exactly the source that was applied.
        status = relay_json("status --flake /home/alice/config --state-dir /home/alice/state")
        assert status["managed_module"] == "in-sync", status

    with subtest("undo restores source and runtime together"):
        report = relay_json("undo --yes --observe 1 --state-dir /home/alice/state")
        assert report["outcome"] == "rolled-back" and report["reason"] == "undo", report
        assert current() == base and profile() == base
        machine.fail("test -e /run/current-system/sw/bin/hello")
        machine.fail("systemctl is-active relay-demo.service")
        assert managed() == open("${managedBase}").read()

    with subtest("the natural-language front end proposes, validates and never confirms for itself"):
        ask = "ask 'install hello' --host machine --flake /home/alice/config --provider command --provider-command ${aiProvider} --state-dir /home/alice/state"
        # Planning from a proposal changes nothing.
        plan = relay_json(ask)
        assert plan["candidate_system"] == good and plan["risk"] == "LIVE_SWITCHABLE", plan
        assert current() == base and profile() == base
        assert managed() == open("${managedBase}").read()
        relay_json(f"discard {plan['id']} --state-dir /home/alice/state")
        # --yes is refused outright; a typed "no" leaves the system untouched.
        status, _, err = relay(ask + " --yes")
        assert status != 0 and "--yes is not accepted" in err, err
        status, _, err = relay(ask + " --apply", env="printf 'no\\n' |")
        assert status != 0 and "not confirmed" in err, err
        assert current() == base and profile() == base
        relay_json("discard " + machine.succeed("ls -1 /home/alice/state/changes | tail -1").strip() + " --state-dir /home/alice/state")
        # Only an interactive "yes" applies it, through the same pipeline.
        status, out, err = relay(ask + " --apply --observe 1", env="printf 'yes\\n' |")
        assert status == 0, (out, err)
        # `ask --apply` prints the plan and then the apply result, one JSON document per line.
        assert json.loads(out.strip().splitlines()[-1])["outcome"] == "switched", out
        assert current() == good
        relay_json("undo --yes --observe 1 --state-dir /home/alice/state")
        assert current() == base and profile() == base

    with subtest("an option change goes through the same pipeline"):
        plan = relay_json("plan --flake /home/alice/config --host machine set-option services.relay-demo.mode string 'fast mode' --state-dir /home/alice/state")
        assert plan["risk"] == "LIVE_SWITCHABLE" and plan["candidate_system"] == option, plan
        assert plan["managed_diff"] == ['+   services.relay-demo.mode = "fast mode";'], plan
        report = relay_json(f"apply {plan['id']} --yes --expect-active relay-option.service --observe 1 --state-dir /home/alice/state")
        assert report["outcome"] == "switched", report
        assert current() == option and profile() == option
        assert machine.succeed("cat /etc/relay-option-demo") == "fast mode"
        relay_json("undo --yes --observe 1 --state-dir /home/alice/state")
        assert current() == base and profile() == base
        assert managed() == open("${managedBase}").read()

    with subtest("a unit that crashes after activation fails the health check and is rolled back"):
        plan = relay_json("plan --flake /home/alice/config --host machine add-package cowsay --state-dir /home/alice/state")
        assert plan["candidate_system"] == broken
        report = relay_json(f"apply {plan['id']} --yes --observe 15 --state-dir /home/alice/state", expect=2)
        assert report["outcome"] == "rolled-back" and report["reason"] == "health-check-failed", report
        assert current() == base and profile() == base
        machine.fail("test -e /run/current-system/sw/bin/cowsay")
        assert managed() == open("${managedBase}").read()
        history = relay_json("history --state-dir /home/alice/state")
        assert history[0]["state"] == "rolled-back", history[0]

    with subtest("a unit whose start job fails during activation is rolled back"):
        plan = relay_json("plan --flake /home/alice/config --host machine set-option services.relay-test.enable bool true --state-dir /home/alice/state")
        assert plan["candidate_system"] == failing
        report = relay_json(f"apply {plan['id']} --yes --observe 1 --state-dir /home/alice/state", expect=2)
        # NixOS reports the failed start job itself (test-activation-failed); a health check that
        # notices first (health-check-failed) is an equally safe outcome.
        assert report["outcome"] == "rolled-back", report
        assert report["reason"] in ("test-activation-failed", "health-check-failed"), report
        assert current() == base and profile() == base
        assert managed() == open("${managedBase}").read()

    with subtest("a switch inhibitor forces the boot path and is never bypassed"):
        plan = relay_json("plan --flake /home/alice/config --host machine add-package figlet --state-dir /home/alice/state")
        assert plan["risk"] == "REBOOT_REQUIRED", plan
        assert plan["inhibitors"] == ["relay-test: 1 -> 2"], plan
        # Cross-check Relay's prediction against NixOS' own pre-switch check.
        machine.fail(f"{inhibited}/bin/switch-to-configuration test")
        assert current() == base
        report = relay_json(
            f"apply {plan['id']} --yes --state-dir /home/alice/state",
            env="NIXOS_NO_CHECK=1",
        )
        assert report["outcome"] == "reboot-pending", report
        assert current() == base, "the running system is untouched until the reboot"
        assert profile() == inhibited, "the next boot generation is prepared"
        status = relay_json("status --state-dir /home/alice/state")
        assert status["unresolved_change"] == plan["id"], status
        # Nothing else may start while the reboot is pending.
        status_code, _, err = relay("plan --flake /home/alice/config --host machine add-package hello --state-dir /home/alice/state")
        assert status_code != 0 and "unresolved" in err, err
        recovered = relay_json("recover --abort-pending --state-dir /home/alice/state", expect=2)
        assert recovered[0]["outcome"] == "rolled-back", recovered
        assert profile() == base and current() == base
        assert managed() == open("${managedBase}").read()

    with subtest("the journal and state stay private and value-free"):
        machine.succeed("test \"$(stat -c %a /home/alice/state/journal)\" = 600")
        machine.fail("grep -q hunter2 /home/alice/state/journal")
        history = relay_json("history --state-dir /home/alice/state")
        states = [entry["state"] for entry in history]
        # Everything is settled: undone, rolled back or discarded; nothing is left in flight.
        assert all(state in ("rolled-back", "failed") for state in states), history
        assert states.count("rolled-back") == 6, history
        assert states.count("failed") == 2, history
  '';
}
