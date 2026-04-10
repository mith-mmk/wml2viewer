## @ command
@animation [loop 0] //  animation start
/path/to
@wait 100 // ms
/path/to
@wait 100
/path/to
@animation end // animation block

option
loop n (n >= 0, loop 0 is infinite)

x = uint
y = uint

@wait int // ms wait
@background color #RRBBGG // set color,
@background image
@noclear // next is clear(override)
@scale // scale option
@resize x,y
@zoom 1.0 // set zoom
@start x, y// next image draw begin (x,y)
@flip // 左右反転
@flap // 上下反転

@(
// wmlscript 
)