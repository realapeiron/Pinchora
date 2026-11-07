"""Minimal lossless RGBA8 PNG IO for source assets; only Python's standard library."""
import struct
import zlib

class Image:
    def __init__(self, size, data=None):
        self.width, self.height = size
        self.size = size
        self.data = bytearray(data) if data is not None else bytearray(self.width*self.height*4)

    @classmethod
    def new(cls, mode, size):
        assert mode == 'RGBA'
        return cls(size)

    @classmethod
    def open(cls, path):
        b = path.read_bytes()
        assert b[:8] == b'\x89PNG\r\n\x1a\n'
        w,h,depth,color,compression,filtering,interlace = struct.unpack('>IIBBBBB', b[16:29])
        assert (depth,color,compression,filtering,interlace) == (8,6,0,0,0), path
        at=8; chunks=[]
        while at<len(b):
            n=struct.unpack('>I',b[at:at+4])[0]; kind=b[at+4:at+8]; payload=b[at+8:at+8+n]
            assert zlib.crc32(kind+payload)&0xffffffff == struct.unpack('>I',b[at+8+n:at+12+n])[0]
            if kind==b'IDAT': chunks.append(payload)
            at+=n+12
        raw=zlib.decompress(b''.join(chunks)); stride=w*4; previous=bytearray(stride); result=bytearray()
        for y in range(h):
            f=raw[y*(stride+1)]; row=bytearray(raw[y*(stride+1)+1:(y+1)*(stride+1)])
            if f:
                for i in range(stride):
                    a=row[i-4] if i>=4 else 0; u=previous[i]; c=previous[i-4] if i>=4 else 0
                    if f==1: predictor=a
                    elif f==2: predictor=u
                    elif f==3: predictor=(a+u)//2
                    elif f==4:
                        p=a+u-c; pa,pb,pc=abs(p-a),abs(p-u),abs(p-c)
                        predictor=a if pa<=pb and pa<=pc else u if pb<=pc else c
                    else: raise ValueError('PNG filter')
                    row[i]=(row[i]+predictor)&255
            result.extend(row); previous=row
        return cls((w,h),result)

    def convert(self, mode):
        assert mode=='RGBA'
        return self

    def crop(self, box):
        x,y,right,bottom=box; w,h=right-x,bottom-y
        assert 0<=x<right<=self.width and 0<=y<bottom<=self.height
        return Image((w,h),b''.join(self.data[((y+r)*self.width+x)*4:((y+r)*self.width+x+w)*4] for r in range(h)))

    def paste(self, image, point):
        x,y=point
        for row in range(image.height):
            start=((y+row)*self.width+x)*4
            self.data[start:start+image.width*4]=image.data[row*image.width*4:(row+1)*image.width*4]

    def tobytes(self):
        return bytes(self.data)

    def save(self, stream, **_options):
        def chunk(kind,payload):
            return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
        stride=self.width*4
        raw=b''.join(b'\0'+self.data[y*stride:(y+1)*stride] for y in range(self.height))
        stream.write(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',self.width,self.height,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw,9))+chunk(b'IEND',b''))
