import json, os, pathlib, socket, subprocess, sys, tempfile, urllib.error, urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))

binary = sys.argv[1]
with tempfile.TemporaryDirectory(prefix='atlas-agent-http-') as temporary:
    root = pathlib.Path(temporary).resolve() / 'data'
    proc = subprocess.Popen([binary, '--root', str(root), '--port', '0'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        proc.stdin.write('owner-test-secret\n'); proc.stdin.flush()
        port = proc.stdout.readline().strip(); assert port.isdigit(), proc.stderr.read()
        def request(path, credential, payload=None, method=None, header='X-Atlas-Token'):
            data = None if payload is None else json.dumps(payload).encode()
            headers = {header:credential}
            if data is not None: headers['Content-Type']='application/json'
            req=urllib.request.Request('http://127.0.0.1:'+port+path,data=data,headers=headers,method=method)
            try:
                with urllib.request.urlopen(req,timeout=15) as response: return response.status,json.load(response)
            except urllib.error.HTTPError as e: return e.code,json.load(e)
        owner='owner-test-secret'
        _,profile=request('/api/agents',owner,{'name':'Agent'})
        key=profile['token']; aid=profile['id']; h='X-Atlas-Agent-Token'
        _,other=request('/api/agents',owner,{'name':'Other'}); otherkey=other['token']
        status,note=request('/api/w/default/notes',owner,{'title':'Test','text':'alpha unique learning'})
        assert status==201,(status,note); nid=note['id']; path=f'/api/agent/default/notes/{nid}'
        assert request(path,key,header=h)[0]==403
        assert request('/api/w/default/notes/'+nid,key,header='X-Atlas-Token')[0]==403
        permissions={'default':{'read':True,'lexical':True}}
        assert request('/api/agents/'+aid,owner,{'name':'Agent','permissions':permissions},method='PUT')[0]==200
        assert request(path,key,header=h)[1]['text']=='alpha unique learning'
        assert request(path,otherkey,header=h)[0]==403
        assert request('/api/agent/workspaces',key,header=h)[1]['workspaces'][0]['id']=='default'
        status,result=request('/api/agent/default/search',key,{'query':'alpha'},header=h)
        assert status==200,(status,result)
        assert any(x['id']==nid for x in result['results']),(status,result)
        status,space=request('/api/workspaces',owner,{'name':'Protected','protected':True,'password':'test-password'})
        assert status==201,(status,space); wid=space['id']
        status,pnote=request('/api/w/'+wid+'/notes',owner,{'title':'Private','text':'protected orange content'})
        assert status==201,(status,pnote); protected_path=f'/api/agent/{wid}/notes/{pnote["id"]}'
        assert request(protected_path,key,header=h)[0]==403
        permissions[wid]={'read':True,'lexical':True}
        assert request('/api/agents/'+aid,owner,{'name':'Agent','permissions':permissions},method='PUT')[0]==200
        assert request(protected_path,key,header=h)[1]['text']=='protected orange content'
        assert any(x['id']==wid and x['available'] for x in request('/api/agent/workspaces',key,header=h)[1]['workspaces'])
        assert request('/api/agent/'+wid+'/search',key,{'query':'orange'},header=h)[0]==200
        assert request('/api/workspaces/'+wid+'/close',owner,{})[0]==200
        assert request(protected_path,key,header=h)[0]==404
        assert any(x['id']==wid and not x['available'] for x in request('/api/agent/workspaces',key,header=h)[1]['workspaces'])
        disk=pathlib.Path(temporary).resolve()/'disk'; disk.mkdir()
        capability=request('/api/workspaces/authorize',owner,{'operation':'export','path':str(disk)})[1]['capability']
        status,made=request('/api/workspaces/create-external',owner,{'capability':capability,'name':'External','protected':False});assert status==201,(status,made)
        status,attached=request('/api/workspaces/attach',owner,{'capability':made['capability']});assert status==200,(status,attached); ext=attached['id']
        status,enote=request('/api/w/'+ext+'/notes',owner,{'title':'External note','text':'external apple content'});assert status==201,(status,enote)
        permissions[ext]={'read':True,'lexical':True}; assert request('/api/agents/'+aid,owner,{'name':'Agent','permissions':permissions},method='PUT')[0]==200
        epath=f'/api/agent/{ext}/notes/{enote["id"]}'
        assert request(epath,key,header=h)[1]['text']=='external apple content'
        assert request('/api/agent/'+ext+'/search',key,{'query':'apple'},header=h)[0]==200
        assert any(x['id']==ext and x['available'] for x in request('/api/agent/workspaces',key,header=h)[1]['workspaces'])
        assert request('/api/workspaces/'+ext+'/close',owner,{})[0]==200
        assert request(epath,key,header=h)[0]==404
        assert any(x['id']==ext and not x['available'] for x in request('/api/agent/workspaces',key,header=h)[1]['workspaces'])
        status,big=request('/api/w/default/notes',owner,{'title':'Large','text':'x'*4000000});assert status==201,(status,big)
        unread=socket.socket();unread.setsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF,1024);unread.settimeout(5);unread.connect(('127.0.0.1',int(port)))
        unread.sendall((f'GET /api/agent/default/notes/{big["id"]} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Atlas-Agent-Token: {key}\r\n\r\n').encode())
        import time;time.sleep(.5)
        assert request('/api/w/default/notes',owner,{'title':'While unread','text':'works'})[0]==201
        assert request('/api/agents/'+aid,owner,{'name':'Agent','permissions':permissions},method='PUT')[0]==200
        unread.close()
        slow=socket.create_connection(('127.0.0.1',int(port)),timeout=5)
        slow.sendall((f'POST /api/agent/default/search HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: 16\r\n\r\n').encode())
        assert request('/api/w/default/notes',owner,{'title':'Parallel','text':'edit works while request is slow'})[0]==201
        assert request('/api/agents/'+aid,owner,method='DELETE')[0]==200
        slow.sendall(b'{"query":"test"}')
        response=slow.recv(4096);slow.close()
        assert b'403' in response,response
        assert request(path,key,header=h)[0]==403
    finally:
        proc.terminate(); proc.wait(timeout=10)
print('agent HTTP checks passed')
