"""Region-based team masks: pick whole UV triangles (by 3D height band + loose colour rule),
rasterise them filled into the mask. Replaces speckle-prone per-texel thresholds for weak assets.
Usage: uvmasks.py <id> ; prints coverage; writes textures/<id>/team-uv.png and a preview albedo."""
import sys, json, struct, io, os
import numpy as np
from PIL import Image, ImageDraw
D = os.path.expanduser('~/artefacts/grubcraft/meshy-core/')
# id -> (hue_lo,hue_hi,sat_min,val_min,hfrac_lo,hfrac_hi,note)
RULES = {
 'V-13': (8,40,0.40,0.35,0.18,0.60,'orange-brown skirt/tabard (no red in texture)'),
 'V-03': (22,48,0.50,0.40,0.15,0.75,'ochre/orange bracers, belt, trim'),
 'V-19': (0,15,0.45,0.35,0.0,1.0,'red door banner (loose red)'),
 'B-03': (15,45,0.55,0.40,0.10,0.90,'orange-gold armour trim'),
 'B-23': (25,42,0.70,0.60,0.45,1.0,'orange veins on pustule'),
}
def load(path):
    b=open(path,'rb').read(); jl,=struct.unpack('<I',b[12:16]); j=json.loads(b[20:20+jl]); bin_=b[20+jl+8:]
    p=j['meshes'][0]['primitives'][0]
    def acc(i):
        a=j['accessors'][i]; bv=j['bufferViews'][a['bufferView']]; o=bv.get('byteOffset',0)+a.get('byteOffset',0)
        n={'SCALAR':1,'VEC2':2,'VEC3':3,'VEC4':4}[a['type']]; dt={5126:'<f4',5125:'<u4',5123:'<u2',5121:'u1'}[a['componentType']]
        stride=bv.get('byteStride'); 
        arr=np.frombuffer(bin_,dtype=dt,count=a['count']*n,offset=o).reshape(a['count'],n) if not stride else None
        return arr
    pos=acc(p['attributes']['POSITION']); uv=acc(p['attributes']['TEXCOORD_0']); idx=acc(p['indices']).reshape(-1,3)
    ti=j['materials'][p.get('material',0)]['pbrMetallicRoughness']['baseColorTexture']['index']; src=j['textures'][ti]['source']
    bv=j['bufferViews'][j['images'][src]['bufferView']]; o=bv.get('byteOffset',0)
    return pos,uv,idx,Image.open(io.BytesIO(bin_[o:o+bv['byteLength']])).convert('RGB')
def hsv(rgb):
    a=rgb/255.; mx=a.max(-1); mn=a.min(-1); d=mx-mn+1e-6; r,g,b=a[...,0],a[...,1],a[...,2]
    h=np.where(mx==r,((g-b)/d)%6,np.where(mx==g,(b-r)/d+2,(r-g)/d+4))*60
    return h,d/(mx+1e-6),mx
def build(aid):
    f=D+'models/%s-anim.glb'%aid
    if not os.path.exists(f): f=D+'models/%s-textured.glb'%aid
    pos,uv,idx,im=load(f); hl,hh,sm,vm,fl,fh,note=RULES[aid]
    W,H=im.size; arr=np.asarray(im)
    z=pos[:,2] if False else pos[:,1]  # glTF is Y-up
    zf=(z-z.min())/(z.max()-z.min())
    # sample colour at each corner and centroid
    tri_uv=uv[idx]            # (T,3,2)
    cen=tri_uv.mean(1)
    px=np.clip((cen[:,0]*(W-1)).astype(int),0,W-1); py=np.clip((cen[:,1]*(H-1)).astype(int),0,H-1)
    col=arr[py,px]; h,s,v=hsv(col.astype(np.float32))
    hc=zf[idx].mean(1)
    sel=(h>=hl)&(h<=hh)&(s>=sm)&(v>=vm)&(hc>=fl)&(hc<=fh)
    m=Image.new('L',(W,H),0); dr=ImageDraw.Draw(m)
    for t in np.where(sel)[0]:
        dr.polygon([(float(u)*(W-1),float(vv)*(H-1)) for u,vv in tri_uv[t]],fill=255)
    return im,m,int(sel.sum()),len(idx),note
if __name__=='__main__':
    for aid in sys.argv[1:]:
        im,m,n,T,note=build(aid)
        os.makedirs(D+'textures/'+aid,exist_ok=True)
        m.convert('RGB').save(D+'textures/%s/team-uv.png'%aid)
        a=np.asarray(im).copy(); mk=np.asarray(m)>0; a[mk]=(0.4*a[mk]+0.6*np.array([255,0,255])).astype(np.uint8)
        Image.fromarray(a).save(D+'textures/%s/preview-mask.png'%aid)
        print(aid,'tris',n,'/',T,'cover %.2f%%'%(np.asarray(m).mean()/255*100),note)
