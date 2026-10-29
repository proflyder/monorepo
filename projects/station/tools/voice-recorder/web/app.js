const $ = id => document.getElementById(id);
const labels = {normal:'Обычный голос',relaxed:'Расслабленно',quiet:'Тихий голос',whisper:'Шёпот',commands:'Команды',background:'Фон'};
const prompts = {normal:'«Рикардо» — 10–15 раз своим обычным голосом.',relaxed:'«Рикардо» — естественно, быстрее или расслабленнее. Не старайся выговаривать идеально.',quiet:'«Рикардо» — 10–15 раз тихим голосом, но не шёпотом.',whisper:'«Рикардо» — настоящим шёпотом, с привычного расстояния.',commands:'«Рикардо, который час?» · «Рикардо, включи музыку» · «Рикардо, стоп» · «Рикардо, сделай потише».',background:'Читай нейтральный текст или запиши бытовые звуки. Не произноси «Рикардо». Не записывай других людей без согласия.'};
let token = '', lastFiles = '', latest = null;
function prompt(){ $('prompt').textContent=prompts[$('block').value]; }
$('block').onchange=prompt;prompt();
async function post(path, body={}){
 try{
  const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json','X-Recorder-Token':token},body:JSON.stringify(body)});
  const data=await r.json();if(!r.ok)throw Error(data.error||'Запрос не выполнен');await poll();
 }catch(e){$('status').textContent=e.message;$('status').classList.add('error');}
}
$('connect').onclick=()=>post('/api/start',{consent:$('consent').checked});
$('disconnect').onclick=()=>post('/api/stop');
$('record').onclick=()=>post('/api/take/start',{block:$('block').value,split:$('split').value,note:$('note').value});
$('finish').onclick=()=>post('/api/take/stop');
const clock=s=>`${Math.floor(s/60)}:${String(Math.floor(s%60)).padStart(2,'0')}`;
function renderFiles(s){
 const key=JSON.stringify([s.files,s.restored]);if(lastFiles===key)return;lastFiles=key;
 $('files').replaceChildren();$('total').textContent=`${s.files.filter(f=>!f.excluded).length} дублей в наборе`;
 for(const f of s.files){
  const item=document.createElement('div');item.className='take'+(f.excluded?' excluded':'');
  const title=document.createElement('strong');title.textContent=`${labels[f.block]||f.block} · ${f.split==='test'?'Проверка':'Обучение'} · ${f.seconds} с`;
  const info=document.createElement('p');info.className='small';info.textContent=[f.created,f.note,f.interrupted?'Запись прервана — проверь целостность':'',f.peak===0?'Нет сигнала':f.peak>=32760?'Возможна перегрузка':'',f.excluded?'Исключён из набора':''].filter(Boolean).join(' · ');
  item.append(title,info);
  const link=document.createElement('a');link.href='/audio/'+f.id;link.download=`ricardo-${f.block}-${f.split}-${f.id}.wav`;link.textContent='Скачать WAV';item.append(link);
  if(s.restored){const audio=document.createElement('audio');audio.controls=true;audio.preload='none';audio.src='/audio/'+f.id;item.append(audio);}
  const exclude=document.createElement('button');exclude.className='secondary';exclude.textContent=f.excluded?'Вернуть в набор':'Исключить дубль';exclude.onclick=()=>post('/api/take/exclude',{id:f.id,excluded:!f.excluded});item.append(exclude);
  $('files').append(item);
 }
}
let polling=false;
async function poll(){
 if(polling)return;polling=true;
 try{
  const r=await fetch('/api/status',{headers:{'X-Recorder-Token':token}});if(!r.ok)throw Error();const s=await r.json();latest=s;
  $('status').textContent=s.last_error?`${s.message} Последняя ошибка: ${s.last_error}`:s.message;$('status').classList.toggle('error',s.state==='error'||!!s.last_error);
  $('badge').textContent={idle:'Обычный режим',preparing:'Подготовка',ready:'Ассистент выключен',stopping:'Восстановление',error:'Нужна проверка'}[s.state];
  $('connect').disabled=!s.restored||!['idle','error'].includes(s.state);
  $('disconnect').disabled=s.restored||s.state==='stopping';
  $('record').disabled=s.state!=='ready'||!!s.take;$('finish').disabled=!s.take;
  for(const id of ['block','split','note'])$(id).disabled=!!s.take;
  $('timer').textContent=clock(s.take?.seconds||0);
  $('db').textContent=s.state==='ready'?`${s.db} dBFS · после обработки колонки`:'Нет активного потока';
  $('level').style.width=(s.state==='ready'?Math.max(0,Math.min(100,(s.db+65)/65*100)):0)+'%';
  $('remaining').textContent=s.restored?'':`Сеанс: осталось ${clock(s.remaining)}`;renderFiles(s);
 }catch(e){$('status').textContent='Нет связи с локальным сервером. Watchdog на колонке завершит сеанс и вернёт Pulsar.';$('status').classList.add('error');$('record').disabled=true;}
 finally{polling=false;}
}
window.addEventListener('pagehide',()=>{if(token&&latest&&!latest.restored)fetch('/api/stop',{method:'POST',headers:{'Content-Type':'application/json','X-Recorder-Token':token},body:'{}',keepalive:true}).catch(()=>{});});
fetch('/api/session-token').then(r=>r.json()).then(data=>{token=data.token;poll();setInterval(poll,1000);});
