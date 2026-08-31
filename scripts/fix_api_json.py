import sys
sys.stdout.reconfigure(encoding='utf-8')

path = r'F:\Projects\claude-code-rust\src\api.ts'
with open(path, 'r', encoding='utf-8') as f:
    c = f.read()

# Fix swarmCreateSession - add .json()
old1 = '''export async function swarmCreateSession(title: string, workspace?: string): Promise<string> {
  const res = await request('/swarm/sessions', {
    method: 'POST',
    body: JSON.stringify({ title, workspace }),
  });
  return (res as any).id;
}'''
new1 = '''export async function swarmCreateSession(title: string, workspace?: string): Promise<string> {
  const res = await request('/swarm/sessions', {
    method: 'POST',
    body: JSON.stringify({ title, workspace }),
  });
  const data = await res.json();
  return data.id;
}'''

if old1 in c:
    c = c.replace(old1, new1)
    print('Fixed swarmCreateSession')

# Fix swarmListSessions
old2 = '''export async function swarmListSessions(): Promise<SwarmSession[]> {
  const res = await request('/swarm/sessions');
  return (res as any).sessions || [];
}'''
new2 = '''export async function swarmListSessions(): Promise<SwarmSession[]> {
  const res = await request('/swarm/sessions');
  const data = await res.json();
  return data.sessions || [];
}'''

if old2 in c:
    c = c.replace(old2, new2)
    print('Fixed swarmListSessions')

# Fix swarmGetSession
old3 = '''export async function swarmGetSession(id: string): Promise<SwarmSession> {
  const res = await request('/swarm/sessions/' + id);
  return res as any;
}'''
new3 = '''export async function swarmGetSession(id: string): Promise<SwarmSession> {
  const res = await request('/swarm/sessions/' + id);
  return await res.json();
}'''

if old3 in c:
    c = c.replace(old3, new3)
    print('Fixed swarmGetSession')

# Fix swarmDeleteSession
old4 = '''export async function swarmDeleteSession(id: string): Promise<void> {
  await request('/swarm/sessions/' + id, { method: 'DELETE' });
}'''
new4 = '''export async function swarmDeleteSession(id: string): Promise<void> {
  const res = await request('/swarm/sessions/' + id, { method: 'DELETE' });
  await res.json().catch(() => {});
}'''

if old4 in c:
    c = c.replace(old4, new4)
    print('Fixed swarmDeleteSession')

# Fix swarmGetMessages
old5 = '''export async function swarmGetMessages(sessionId: string): Promise<SwarmMessage[]> {
  const res = await request('/swarm/sessions/' + sessionId + '/messages');
  return (res as any).messages || [];
}'''
new5 = '''export async function swarmGetMessages(sessionId: string): Promise<SwarmMessage[]> {
  const res = await request('/swarm/sessions/' + sessionId + '/messages');
  const data = await res.json();
  return data.messages || [];
}'''

if old5 in c:
    c = c.replace(old5, new5)
    print('Fixed swarmGetMessages')

# Fix swarmAddMessage
old6 = '''  const res = await request('/swarm/sessions/' + sessionId + '/messages', {
    method: 'POST',
    body: JSON.stringify({ role, content, agent_name: agentName, agent_icon: agentIcon, agent_color: agentColor, type: msgType }),
  });
  return (res as any).id;'''
new6 = '''  const res = await request('/swarm/sessions/' + sessionId + '/messages', {
    method: 'POST',
    body: JSON.stringify({ role, content, agent_name: agentName, agent_icon: agentIcon, agent_color: agentColor, type: msgType }),
  });
  const data = await res.json();
  return data.id;'''

if old6 in c:
    c = c.replace(old6, new6)
    print('Fixed swarmAddMessage')

# Fix swarmUpdateStatus
old7 = '''export async function swarmUpdateStatus(
  sessionId: string,
  status: string,
  agentStatus?: Record<string, string>,
): Promise<void> {
  await request('/swarm/sessions/' + sessionId + '/status', {
    method: 'POST',
    body: JSON.stringify({ status, agent_status: agentStatus ? JSON.stringify(agentStatus) : undefined }),
  });
}'''
new7 = '''export async function swarmUpdateStatus(
  sessionId: string,
  status: string,
  agentStatus?: Record<string, string>,
): Promise<void> {
  const res = await request('/swarm/sessions/' + sessionId + '/status', {
    method: 'POST',
    body: JSON.stringify({ status, agent_status: agentStatus ? JSON.stringify(agentStatus) : undefined }),
  });
  await res.json().catch(() => {});
}'''

if old7 in c:
    c = c.replace(old7, new7)
    print('Fixed swarmUpdateStatus')

with open(path, 'w', encoding='utf-8') as f:
    f.write(c)
print('All swarm API functions fixed')
