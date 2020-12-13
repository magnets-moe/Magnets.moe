#!/usr/bin/python

from fontforge import *

f = font()
f.fontname = 'magnets'
f.familyname = 'magnets'
f.fullname = 'magnets'

for c in ['M', 'T']:
    glyph = f.createChar(ord(c))
    glyph.importOutlines(f'letters/{c}.svg')

f.generate('magnets.woff2')
