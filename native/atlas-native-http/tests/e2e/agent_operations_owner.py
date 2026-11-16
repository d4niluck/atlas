"""Verify owner authentication and the public shape of the active-operation summary."""
import json, os, pathlib, subprocess, sys, tempfile, urllib.error, urllib.request

urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-operations-') as tmp:
    server = subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()), '--root', tmp, '--port', '0'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        server.stdin.write('owner-test-token\n'); server.stdin.flush()
        port = server.stdout.readline().strip()
        assert port.isdigit(), (port, server.stderr.read())
        url = f'http://127.0.0.1:{port}/api/agent-operations'
        try:
            urllib.request.urlopen(url, timeout=5)
            raise AssertionError('unauthenticated request succeeded')
        except urllib.error.HTTPError as err:
            assert err.code == 403, err.code
        request = urllib.request.Request(url, headers={'X-Atlas-Token': 'owner-test-token'})
        with urllib.request.urlopen(request, timeout=5) as response:
            assert response.status == 200
            assert json.load(response) == {'uploads': 0}
        print('owner authentication and empty operation summary verified')
    finally:
        server.terminate()
        server.wait(timeout=10)
