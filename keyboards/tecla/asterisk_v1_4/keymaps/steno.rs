// Position i is the i-th pin in examples/default.rs; the value is the steno
// bit (Plover order: 0=# 1=S- 2=T- 3=K- 4=P- 5=W- 6=H- 7=R- 8=A 9=O 10=*
// 11=E 12=U 13=-F 14=-R 15=-P 16=-B 17=-L 18=-G 19=-T 20=-S 21=-D 22=-Z).
// Pin order mapped on hardware with the asterisk_v1_4_keytest profile.
steno! {
    layer!(Base,
        0, 0,                   // # #
        1, 2, 4, 6,             // top:    S- T- P- H-
        10, 10,                 // * *     (tall, span both rows)
        13, 15, 17, 19, 21,     // top:    -F -P -L -T -D
        1, 3, 5, 7,             // bottom: S- K- W- R-
        14, 16, 18, 20, 22,     // bottom: -R -B -G -S -Z
        8, 9, 11, 12            // thumbs: A O E U
    ),
}
