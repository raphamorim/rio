---
title: 'Sixel protocol'
language: 'en'
---

Sixel, short for "six pixels", is a bitmap graphics format supported by terminals and printers from DEC. It consists of a pattern six pixels high and one wide, resulting in 64 possible patterns. Each possible pattern is assigned an ASCII character, making the sixels easy to transmit on 7-bit serial links.

> Demo of sixel on Rio.

![Demo sixel as gif](/assets/features/demo-sixel.gif)

Sixel was first introduced as a way of sending bitmap graphics to DEC dot matrix printers like the LA50. After being put into "sixel mode" the following data was interpreted to directly control six of the pins in the nine-pin print head. A string of sixel characters encodes a single 6-pixel high row of the image.

![Demo sixel with timg](/assets/features/demo-sixel-2.png)

The system was later re-used as a way to send bitmap data to the VT200 series and VT320 terminals when defining custom character sets. A series of sixels are used to transfer the bitmap for each character. This feature is known as soft character sets or dynamically redefinable character sets (DRCS). With the VT240, VT241, VT330, and VT340, the terminals could decode a complete sixel image to the screen, like those previously sent to printers.

![Demo sixel](/assets/features/demo-sixel.png)

For more information on Sixel: [en.wikipedia.org/wiki/Sixel](https://en.wikipedia.org/wiki/Sixel)