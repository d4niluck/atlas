export const MIN_IMAGE_CARD_WIDTH=230;
export const MAX_IMAGE_CARD_WIDTH=920;

export function imageCardWidth(value:number|undefined|null):number {
 return typeof value==='number'&&Number.isFinite(value)
  ? Math.max(MIN_IMAGE_CARD_WIDTH,Math.min(MAX_IMAGE_CARD_WIDTH,value))
  : MIN_IMAGE_CARD_WIDTH;
}

export function stepImageCardWidth(value:number|undefined|null, direction:-1|1):number {
 return imageCardWidth(imageCardWidth(value)+direction*115);
}

// Before a collapsed stack mounts, reserve the maximum image height; ResizeObserver
// replaces this estimate with the actual dimensions after loading.
export function canvasCardSize(item:{kind:string;title:string;imageExpanded?:boolean;imageWidth?:number|null},card?:Pick<HTMLElement,'offsetWidth'|'offsetHeight'>|null){
 const expandedImage=item.kind==='file'&&!!item.imageExpanded&&/\.(png|jpe?g|gif|webp)$/i.test(item.title);
 const width=expandedImage?imageCardWidth(item.imageWidth):230;
 return {width:card?.offsetWidth||width,height:card?.offsetHeight||(expandedImage?Math.ceil(width*0.9565)+34:82)};
}

// Keep the opposite edge fixed when resizing from the left.
export function resizedImageGeometry(width:number,x:number,delta:number,side:'left'|'right') {
 const start=imageCardWidth(width);
 const next=imageCardWidth(start+(side==='left'?-delta:delta));
 return {width:next,x:side==='left'?x+start-next:x};
}

// Project the pointer displacement onto the card diagonal while preserving its aspect ratio.
export function cornerResizeWidth(width:number,height:number,dx:number,dy:number,side:'left'|'right',vertical:'top'|'bottom') {
 const w=imageCardWidth(width);const h=Math.max(1,height);
 const horizontal=side==='left'?-dx:dx;
 const verticalDelta=(vertical==='top'?-dy:dy)*w/h;
 return imageCardWidth(w+(horizontal+verticalDelta)/2);
}
export function anchoredImagePosition(x:number,y:number,oldWidth:number,newWidth:number,oldHeight:number,newHeight:number,side:'left'|'right',vertical:'top'|'bottom') {
 return {x:side==='left'?x+oldWidth-newWidth:x,y:vertical==='top'?y+oldHeight-newHeight:y};
}
