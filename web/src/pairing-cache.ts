export const pairingKey='doubao-microphone-pairing';
export type Ticket={ticket_id:string;qr_svg:string;url:string;phone_url:string;expiresAt:number};
export type PairingState={owner:string;state:'ticket';ticket:Ticket}|{owner:string;state:'paired'};
type Store=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
export function readPairing(store:Store,owner:string):PairingState|null{
  try{
    const value=JSON.parse(store.getItem(pairingKey)||'null');
    if(value?.owner!==owner)return null;
    if(value.state==='paired')return {owner,state:'paired'};
    const ticket=value.ticket;
    if(value.state==='ticket'&&ticket&&['ticket_id','qr_svg','url','phone_url'].every(key=>typeof ticket[key]==='string')&&Number.isFinite(ticket.expiresAt))return value;
  }catch{/* Discard an interrupted or older cache entry. */}
  return null;
}
export function saveTicket(store:Store,owner:string,response:Omit<Ticket,'expiresAt'>&{expires_in:number},now=Date.now()):PairingState{
  if(!Number.isFinite(response.expires_in)||response.expires_in<=0)throw Error('二维码有效期无效');
  const value:PairingState={owner,state:'ticket',ticket:{ticket_id:response.ticket_id,qr_svg:response.qr_svg,url:response.url,phone_url:response.phone_url,expiresAt:now+response.expires_in*1000}};
  store.setItem(pairingKey,JSON.stringify(value));return value;
}
export function savePaired(store:Store,owner:string):PairingState{
  const value:PairingState={owner,state:'paired'};store.setItem(pairingKey,JSON.stringify(value));return value;
}
export function secondsLeft(ticket:Ticket,now=Date.now()):number{return Math.max(0,Math.ceil((ticket.expiresAt-now)/1000))}
