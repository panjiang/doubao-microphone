import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readPairing,saveTicket,savePaired,secondsLeft,pairingKey} from './pairing-cache.js';
function store(){const map=new Map<string,string>();return {getItem:(key:string)=>map.get(key)??null,setItem:(key:string,value:string)=>{map.set(key,value)},removeItem:(key:string)=>{map.delete(key)}}}
const ticket={ticket_id:'example',qr_svg:'<svg/>',url:'http://127.0.0.1/setup',phone_url:'https://127.0.0.1/',expires_in:300};
test('returning from Settings reuses the ticket and its original expiry',()=>{
  const storage=store();saveTicket(storage,'owner',ticket,1000);
  const cached=readPairing(storage,'owner');assert.equal(cached?.state,'ticket');
  if(cached?.state==='ticket'){assert.equal(cached.ticket.ticket_id,'example');assert.equal(secondsLeft(cached.ticket,2000),299);assert.equal(secondsLeft(cached.ticket,301000),0)}
});
test('an expired ticket stays identifiable until the user refreshes',()=>{
  const storage=store();saveTicket(storage,'owner',ticket,0);
  const cached=readPairing(storage,'owner');assert.equal(cached?.state,'ticket');
  if(cached?.state==='ticket')assert.equal(secondsLeft(cached.ticket,400000),0);
});
test('app restart cannot reuse another admin session ticket or success state',()=>{
  const storage=store();saveTicket(storage,'old',ticket);assert.equal(readPairing(storage,'new'),null);
  savePaired(storage,'old');assert.equal(readPairing(storage,'new'),null);
  assert.equal(readPairing(storage,'old')?.state,'paired');
});
test('damaged cache is ignored',()=>{
  const storage=store();storage.setItem(pairingKey,'broken');assert.equal(readPairing(storage,'owner'),null);
  storage.setItem(pairingKey,JSON.stringify({owner:'owner',state:'ticket',ticket:{...ticket,expiresAt:'tomorrow'}}));assert.equal(readPairing(storage,'owner'),null);
});
