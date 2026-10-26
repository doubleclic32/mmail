// Banc du garde « survol sombre MMEDIA » : reproduit Qt.rgba, une color Qt
// (r/g/b/a lisibles), et le fait qu'une CHAÎNE n'a pas de .r/.g/.b.
function rgba(r,g,b,a){
  const c=(x)=> (x===undefined||Number.isNaN(x))?0:x
  return {r:c(r),g:c(g),b:c(b),a:a,toString(){return "rgba"}}
}
function col(hex){ // une vraie color Qt
  const n=parseInt(hex.slice(1),16)
  return {r:((n>>16)&255)/255,g:((n>>8)&255)/255,b:(n&255)/255,a:1}
}
const CAS = {
  "0.6.20":   (e)=> e,                 // encreEntete rend la CHAÎNE
  "0.6.21":   (e)=> col("#ffffff"),    // Qt.lighter(chaîne,1.0) -> color
}
function survol(version, cles, marque, palTexte, accent, sombre){
  const valeurOu=(k,d)=> cles[k]!==undefined?cles[k]:d
  const demande = sombre ? valeurOu("survolBandeau",0) : valeurOu("survolBande",0)
  if (marque===null || demande<=0){
    let e = sombre && marque ? marque.texteBandeau : palTexte
    if (sombre && marque) e = CAS[version](e)
    return rgba(e.r,e.g,e.b, sombre && marque ? 0.14 : 0.08)
  }
  return sombre ? rgba(1,1,1,demande) : rgba(accent.r,accent.g,accent.b,demande)
}
// Le garde tel qu'écrit dans Main.qml
function garde(sv){ return !(sv.a.toFixed(2)!=="0.14" || sv.r!==sv.g || sv.g!==sv.b) }

const mmedia={texteBandeau:"#ffffff"}
const accentMM=col("#21abe3")
const texteMM=col("#231f20")
const clesPomme={survolBandeau:0.18,survolBande:0.14}

const essais=[
 ["0.6.20 MMEDIA intact", survol("0.6.20",{},mmedia,texteMM,accentMM,true), true],
 ["0.6.21 MMEDIA corrigé", survol("0.6.21",{},mmedia,texteMM,accentMM,true), true],
 ["MUTANT: Pomme fuit sur MMEDIA (0.6.20)", survol("0.6.20",clesPomme,mmedia,texteMM,accentMM,true), false],
 ["MUTANT: Pomme fuit sur MMEDIA (0.6.21)", survol("0.6.21",clesPomme,mmedia,texteMM,accentMM,true), false],
 ["MUTANT: alpha passée à 0.20", rgba(1,1,1,0.20), false],
 ["MUTANT: teinte accent au lieu du gris", rgba(accentMM.r,accentMM.g,accentMM.b,0.14), false],
]
let ko=0
for (const [nom,sv,attendu] of essais){
  const v=garde(sv)
  const ok = v===attendu
  if(!ok) ko++
  console.log((ok?"OK  ":"ECHEC")+" | "+nom+" -> garde "+(v?"vert":"ROUGE")+
    " (a="+sv.a+" r="+sv.r+" g="+sv.g+" b="+sv.b+")")
}
console.log(ko===0 ? "BANC VERT : le garde déclenche sur les 4 mutants et passe sur les 2 versions" : "BANC ROUGE")
process.exit(ko?1:0)
