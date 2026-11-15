//! Underground fluids (`gtfluids`) vs golden vectors from the verbatim GT 5.09.54.265 classes
//! (`XSTR`, `GTUODimension`, `GTUOFluid`, `GTUtility.powInt`, body of `UndergroundOil.getPristineAmount`) compiled
//! against Guava 17.0 sources (MC 1.7.10's Guava) with Forge config/fluid shims, OpenJDK 21 (HotSpot x86_64).
use rwg2_biomes::gtfluids::*;

/// `seed dim chunkX chunkZ registry veinAverage amount`
const GOLDEN: &str = "
0 0 -9 -7 liquid_medium_oil 16721 19046
0 0 -9 -3 liquid_medium_oil 16721 17998
0 0 -9 1 oil 202914 203144
0 0 -9 5 oil 202914 177739
0 0 -9 7 oil 202914 186595
0 0 -9 9 liquid_light_oil 50000 54796
0 0 -6 -7 liquid_light_oil 50000 39499
0 0 -6 -3 liquid_light_oil 50000 45631
0 0 -6 1 liquid_medium_oil 957789 932608
0 0 -6 5 liquid_medium_oil 957789 737080
0 0 -6 9 oil 1501 1831
0 0 -3 -7 liquid_light_oil 50000 58022
0 0 -3 -3 liquid_light_oil 50000 54966
0 0 -3 1 liquid_medium_oil 957789 875226
0 0 -3 3 liquid_medium_oil 957789 1056314
0 0 -3 5 liquid_medium_oil 957789 719928
0 0 -3 9 oil 1501 1213
0 0 0 -7 liquid_medium_oil 6 5
0 0 0 -3 liquid_medium_oil 6 7
0 0 0 1 oil 126187 107767
0 0 0 5 oil 126187 140722
0 0 0 9 liquid_heavy_oil 0 0
0 0 3 -7 liquid_medium_oil 6 6
0 0 3 -3 liquid_medium_oil 6 5
0 0 3 -1 liquid_medium_oil 6 5
0 0 3 1 oil 126187 144343
0 0 3 5 oil 126187 114512
0 0 3 9 liquid_heavy_oil 0 0
0 0 6 -7 liquid_medium_oil 6 7
0 0 6 -3 liquid_medium_oil 6 5
0 0 6 1 oil 126187 115651
0 0 6 5 oil 126187 143279
0 0 6 9 liquid_heavy_oil 0 0
0 0 9 -7 gas_natural_gas 252100 243139
0 0 9 -5 gas_natural_gas 252100 224833
0 0 9 -3 gas_natural_gas 252100 210089
0 0 9 1 gas_natural_gas 50000 37629
0 0 9 5 gas_natural_gas 50000 40989
0 0 9 9 liquid_heavy_oil 24174 21236
0 0 -358153 -29687 liquid_medium_oil 17081 12966
0 0 -495566 -93394 gas_natural_gas 50000 56071
0 0 136569 998473 liquid_heavy_oil 384231 381608
0 0 -654438 861234 liquid_medium_oil 5150 6250
0 0 -631404 -987811 oil 1563 1830
0 0 -74624 -279668 liquid_light_oil 74034 92438
0 0 -710690 -820901 oil 3026 2729
0 0 665298 -579847 gas_natural_gas 50000 62339
0 0 -286795 -257146 oil 405 397
0 0 -388637 683955 liquid_heavy_oil 77 81
0 0 -861925 -797166 gas_natural_gas 50000 47729
0 7 -9 7 liquid_medium_oil 832312 920961
0 7 -6 5 liquid_heavy_oil 1292520 1255125
0 7 -3 3 liquid_heavy_oil 1292520 1340892
0 7 0 1 oil 35664 27238
0 7 3 -1 liquid_heavy_oil 207526 187226
0 7 6 -3 liquid_heavy_oil 207526 173315
0 7 9 -5 liquid_heavy_oil 12842 14928
0 7 213477 -579263 liquid_light_oil 50000 45943
0 7 -336400 -543849 liquid_medium_oil 76 71
0 7 297001 39797 liquid_light_oil 101235 114487
0 -1 -9 7 liquid_light_oil 50000 50592
0 -1 -6 5 liquid_heavy_oil 46 57
0 -1 -3 3 liquid_heavy_oil 46 43
0 -1 0 1 oil 202914 215585
0 -1 3 -1 liquid_medium_oil 16721 13075
0 -1 6 -3 liquid_medium_oil 16721 13089
0 -1 9 -5 liquid_light_oil 50000 57479
0 -1 -301759 -635292 liquid_heavy_oil 19984 22733
0 -1 -446326 313232 oil 27716 32024
0 -1 383450 -452431 liquid_light_oil 329707 272672
1 0 -9 -7 liquid_light_oil 50000 42238
1 0 -9 -3 liquid_light_oil 50000 61432
1 0 -9 1 liquid_medium_oil 957789 862310
1 0 -9 5 liquid_medium_oil 957789 913294
1 0 -9 7 liquid_medium_oil 957789 1150281
1 0 -9 9 oil 1501 1641
1 0 -6 -7 liquid_medium_oil 6 6
1 0 -6 -3 liquid_medium_oil 6 7
1 0 -6 1 oil 126187 151109
1 0 -6 5 oil 126187 129766
1 0 -6 9 liquid_heavy_oil 0 0
1 0 -3 -7 liquid_medium_oil 6 5
1 0 -3 -3 liquid_medium_oil 6 6
1 0 -3 1 oil 126187 111755
1 0 -3 3 oil 126187 152995
1 0 -3 5 oil 126187 109317
1 0 -3 9 liquid_heavy_oil 0 0
1 0 0 -7 gas_natural_gas 252100 304283
1 0 0 -3 gas_natural_gas 252100 243869
1 0 0 1 gas_natural_gas 50000 37648
1 0 0 5 gas_natural_gas 50000 37503
1 0 0 9 liquid_heavy_oil 24174 23611
1 0 3 -7 gas_natural_gas 252100 262398
1 0 3 -3 gas_natural_gas 252100 207504
1 0 3 -1 gas_natural_gas 252100 230428
1 0 3 1 gas_natural_gas 50000 50757
1 0 3 5 gas_natural_gas 50000 51631
1 0 3 9 liquid_heavy_oil 24174 28714
1 0 6 -7 gas_natural_gas 252100 236737
1 0 6 -3 gas_natural_gas 252100 226030
1 0 6 1 gas_natural_gas 50000 41917
1 0 6 5 gas_natural_gas 50000 58578
1 0 6 9 liquid_heavy_oil 24174 20996
1 0 9 -7 liquid_heavy_oil 760564 731773
1 0 9 -5 liquid_heavy_oil 760564 698713
1 0 9 -3 liquid_heavy_oil 760564 583894
1 0 9 1 liquid_medium_oil 81 61
1 0 9 5 liquid_medium_oil 81 71
1 0 9 9 liquid_medium_oil 44650 39121
1 0 624490 -281924 oil 59435 63341
1 0 -230379 37699 liquid_light_oil 50000 45318
1 0 -239472 -281729 liquid_medium_oil 1990035 2481596
1 0 -140705 881856 liquid_heavy_oil 16 19
1 0 -225984 135436 oil 1561 1326
1 0 105288 212028 oil 9 7
1 0 451511 -660638 liquid_heavy_oil 170048 196420
1 0 -437321 -540905 liquid_heavy_oil 2415053 1967019
1 0 197994 -48158 liquid_medium_oil 1384245 1528465
1 0 972701 588275 oil 28749 34324
1 0 15307 -120643 liquid_light_oil 556252 686495
1 7 -9 7 liquid_heavy_oil 1292520 986406
1 7 -6 5 oil 35664 30005
1 7 -3 3 oil 35664 38767
1 7 0 1 liquid_light_oil 333959 254747
1 7 3 -1 liquid_heavy_oil 12842 11931
1 7 6 -3 liquid_heavy_oil 12842 13950
1 7 9 -5 oil 7930 9136
1 7 469957 -260651 gas_natural_gas 50000 53835
1 7 485440 900340 liquid_medium_oil 2753 3393
1 7 16709 161266 oil 2318747 2324546
1 -1 -9 7 liquid_heavy_oil 46 39
1 -1 -6 5 oil 202914 216670
1 -1 -3 3 oil 202914 186729
1 -1 0 1 liquid_medium_oil 957789 1014826
1 -1 3 -1 liquid_light_oil 50000 38087
1 -1 6 -3 liquid_light_oil 50000 52285
1 -1 9 -5 liquid_medium_oil 6 6
1 -1 298883 -209862 oil 5393 5568
1 -1 41954 -606111 liquid_medium_oil 664935 604021
1 -1 -707976 195786 gas_natural_gas 50000 42233
1337 0 -9 -9 gas_natural_gas 220257 240395
1337 0 -9 -7 gas_natural_gas 50000 37981
1337 0 -9 -5 gas_natural_gas 50000 53519
1337 0 -9 -3 gas_natural_gas 50000 62311
1337 0 -9 -1 gas_natural_gas 50000 47541
1337 0 -9 1 liquid_heavy_oil 148294 133312
1337 0 -9 3 liquid_heavy_oil 148294 153592
1337 0 -9 5 liquid_heavy_oil 148294 115856
1337 0 -9 7 liquid_heavy_oil 148294 138194
1337 0 -9 9 liquid_heavy_oil 652978 644950
1337 0 -6 -9 liquid_heavy_oil 2 1
1337 0 -6 -7 liquid_medium_oil 46031 46047
1337 0 -6 -5 liquid_medium_oil 46031 34638
1337 0 -6 -3 liquid_medium_oil 46031 56008
1337 0 -6 -1 liquid_medium_oil 46031 53420
1337 0 -6 1 gas_natural_gas 50000 38502
1337 0 -6 3 gas_natural_gas 50000 48700
1337 0 -6 5 gas_natural_gas 50000 62473
1337 0 -6 7 gas_natural_gas 50000 58361
1337 0 -6 9 liquid_medium_oil 392932 486147
1337 0 -3 -9 liquid_heavy_oil 2 2
1337 0 -3 -7 liquid_medium_oil 46031 57331
1337 0 -3 -5 liquid_medium_oil 46031 48606
1337 0 -3 -3 liquid_medium_oil 46031 44102
1337 0 -3 -1 liquid_medium_oil 46031 40990
1337 0 -3 1 gas_natural_gas 50000 48035
1337 0 -3 3 gas_natural_gas 50000 55386
1337 0 -3 5 gas_natural_gas 50000 58570
1337 0 -3 7 gas_natural_gas 50000 38090
1337 0 -3 9 liquid_medium_oil 392932 390088
1337 0 0 -9 liquid_medium_oil 739458 860431
1337 0 0 -7 liquid_light_oil 50000 37691
1337 0 0 -5 liquid_light_oil 50000 50539
1337 0 0 -3 liquid_light_oil 50000 43166
1337 0 0 -1 liquid_light_oil 50000 38803
1337 0 0 1 liquid_heavy_oil 945326 1052881
1337 0 0 3 liquid_heavy_oil 945326 850084
1337 0 0 5 liquid_heavy_oil 945326 1096565
1337 0 0 7 liquid_heavy_oil 945326 774036
1337 0 0 9 gas_natural_gas 1123036 1237913
1337 0 3 -9 liquid_medium_oil 739458 646423
1337 0 3 -7 liquid_light_oil 50000 57049
1337 0 3 -5 liquid_light_oil 50000 41413
1337 0 3 -3 liquid_light_oil 50000 45933
1337 0 3 -1 liquid_light_oil 50000 55348
1337 0 3 1 liquid_heavy_oil 945326 763457
1337 0 3 3 liquid_heavy_oil 945326 866917
1337 0 3 5 liquid_heavy_oil 945326 1064049
1337 0 3 7 liquid_heavy_oil 945326 772501
1337 0 3 9 gas_natural_gas 1123036 868718
1337 0 6 -9 liquid_medium_oil 739458 693198
1337 0 6 -7 liquid_light_oil 50000 45345
1337 0 6 -5 liquid_light_oil 50000 55763
1337 0 6 -3 liquid_light_oil 50000 55721
1337 0 6 -1 liquid_light_oil 50000 50379
1337 0 6 1 liquid_heavy_oil 945326 773575
1337 0 6 3 liquid_heavy_oil 945326 720096
1337 0 6 5 liquid_heavy_oil 945326 1010194
1337 0 6 7 liquid_heavy_oil 945326 904258
1337 0 6 9 gas_natural_gas 1123036 1086093
1337 0 9 -9 liquid_heavy_oil 377342 333824
1337 0 9 -7 gas_natural_gas 50000 55012
1337 0 9 -5 gas_natural_gas 50000 48731
1337 0 9 -3 gas_natural_gas 50000 40491
1337 0 9 -1 gas_natural_gas 50000 43707
1337 0 9 1 liquid_medium_oil 142 175
1337 0 9 3 liquid_medium_oil 142 175
1337 0 9 5 liquid_medium_oil 142 154
1337 0 9 7 liquid_medium_oil 142 160
1337 0 9 9 liquid_light_oil 1189147 987807
1337 0 -521059 375239 liquid_heavy_oil 360657 363470
1337 0 730078 -280131 liquid_heavy_oil 2563692 2874085
1337 0 -66903 926648 liquid_light_oil 181479 146081
1337 0 113958 145962 gas_natural_gas 844230 664143
1337 0 -747424 -16353 liquid_medium_oil 34613 39507
1337 0 689939 172911 liquid_light_oil 50000 46448
1337 0 -628491 900098 oil 2335449 2799962
1337 0 323848 -389416 oil 157745 167792
1337 0 625561 174502 oil 1311826 1326301
1337 0 966699 -942075 oil 281296 281492
1337 0 250165 -876866 liquid_medium_oil 759466 594462
1337 0 -757834 -270230 gas_natural_gas 128947 133878
1337 0 365251 372415 oil 91277 77036
1337 0 -446502 -352783 liquid_medium_oil 22511 27544
1337 0 -466614 -209201 oil 9376 7397
1337 0 -290097 -521158 liquid_heavy_oil 158831 182828
1337 0 -915980 466651 oil 30 35
1337 0 -374147 -705790 oil 67 80
1337 0 963405 -842297 oil 506200 461183
1337 0 -319808 -984671 liquid_medium_oil 698 569
1337 7 -9 7 liquid_heavy_oil 277 234
1337 7 -6 5 oil 0 0
1337 7 -3 3 oil 0 0
1337 7 0 1 liquid_light_oil 50000 56692
1337 7 3 -1 liquid_medium_oil 454 507
1337 7 6 -3 liquid_medium_oil 454 481
1337 7 9 -5 liquid_heavy_oil 31 28
1337 7 232668 411821 oil 35 28
1337 7 -612706 675610 liquid_light_oil 236889 198039
1337 7 -427677 -134794 liquid_medium_oil 20274 18104
1337 -1 -9 7 liquid_medium_oil 15233 15646
1337 -1 -6 5 gas_natural_gas 50000 57958
1337 -1 -3 3 gas_natural_gas 50000 40201
1337 -1 0 1 liquid_heavy_oil 148294 166045
1337 -1 3 -1 gas_natural_gas 50000 59691
1337 -1 6 -3 gas_natural_gas 50000 47557
1337 -1 9 -5 liquid_medium_oil 46031 56733
1337 -1 -470902 640031 liquid_medium_oil 1112791 971573
1337 -1 682905 -860523 liquid_light_oil 102542 80805
1337 -1 -982191 735784 oil 239668 240104
-1 0 -9 -7 liquid_light_oil 50000 38607
-1 0 -9 -3 liquid_light_oil 50000 51496
-1 0 -9 1 liquid_heavy_oil 46 46
-1 0 -9 5 liquid_heavy_oil 46 44
-1 0 -9 7 liquid_heavy_oil 46 39
-1 0 -9 9 gas_natural_gas 378703 376849
-1 0 -6 -7 liquid_medium_oil 16721 20501
-1 0 -6 -3 liquid_medium_oil 16721 12886
-1 0 -6 1 oil 202914 242783
-1 0 -6 5 oil 202914 216670
-1 0 -6 9 liquid_light_oil 50000 48894
-1 0 -3 -7 liquid_medium_oil 16721 18161
-1 0 -3 -3 liquid_medium_oil 16721 19787
-1 0 -3 1 oil 202914 234573
-1 0 -3 3 oil 202914 186729
-1 0 -3 5 oil 202914 166132
-1 0 -3 9 liquid_light_oil 50000 50783
-1 0 0 -7 liquid_light_oil 50000 47747
-1 0 0 -3 liquid_light_oil 50000 61505
-1 0 0 1 liquid_medium_oil 957789 1014826
-1 0 0 5 liquid_medium_oil 957789 1167681
-1 0 0 9 oil 1501 1849
-1 0 3 -7 liquid_light_oil 50000 37672
-1 0 3 -3 liquid_light_oil 50000 61611
-1 0 3 -1 liquid_light_oil 50000 38087
-1 0 3 1 liquid_medium_oil 957789 757237
-1 0 3 5 liquid_medium_oil 957789 1133562
-1 0 3 9 oil 1501 1556
-1 0 6 -7 liquid_light_oil 50000 57685
-1 0 6 -3 liquid_light_oil 50000 52285
-1 0 6 1 liquid_medium_oil 957789 1119481
-1 0 6 5 liquid_medium_oil 957789 801552
-1 0 6 9 oil 1501 1283
-1 0 9 -7 liquid_medium_oil 6 5
-1 0 9 -5 liquid_medium_oil 6 6
-1 0 9 -3 liquid_medium_oil 6 5
-1 0 9 1 oil 126187 130391
-1 0 9 5 oil 126187 141845
-1 0 9 9 liquid_heavy_oil 0 0
-1 0 -980543 -448089 liquid_heavy_oil 373157 298090
-1 0 529268 651625 gas_natural_gas 1298607 1030185
-1 0 -146331 278900 liquid_light_oil 367989 285487
-1 0 507132 -630628 gas_natural_gas 50000 57546
-1 0 -756113 -541919 gas_natural_gas 50000 50707
-1 0 -424579 -354144 gas_natural_gas 328573 293560
-1 0 -715947 91894 liquid_medium_oil 0 0
-1 0 187849 106268 liquid_medium_oil 1389132 1231444
-1 0 -398153 -562387 liquid_heavy_oil 0 0
-1 0 475974 202869 oil 31518 28866
-1 0 -594846 555323 gas_natural_gas 50000 42297
-1 7 -9 7 gas_natural_gas 167376 134074
-1 7 -6 5 liquid_medium_oil 832312 977997
-1 7 -3 3 liquid_medium_oil 832312 807389
-1 7 0 1 liquid_heavy_oil 1292520 1311493
-1 7 3 -1 liquid_medium_oil 22257 18660
-1 7 6 -3 liquid_medium_oil 22257 27658
-1 7 9 -5 liquid_heavy_oil 207526 181726
-1 7 -858267 690973 liquid_light_oil 50000 44690
-1 7 689851 781759 liquid_light_oil 50000 40980
-1 7 917275 616978 gas_natural_gas 50000 50957
-1 -1 -9 7 oil 44297 36919
-1 -1 -6 5 liquid_light_oil 50000 45892
-1 -1 -3 3 liquid_light_oil 50000 51079
-1 -1 0 1 liquid_heavy_oil 46 37
-1 -1 3 -1 liquid_light_oil 50000 48531
-1 -1 6 -3 liquid_light_oil 50000 56941
-1 -1 9 -5 liquid_medium_oil 16721 15671
-1 -1 -39392 -432037 liquid_medium_oil 237 238
-1 -1 502431 3912 liquid_light_oil 149238 143794
-1 -1 -897431 962740 gas_natural_gas 50000 42545
42 0 -9 -7 oil 594593 449569
42 0 -9 -3 oil 594593 601928
42 0 -9 1 liquid_light_oil 50000 60113
42 0 -9 5 liquid_light_oil 50000 50433
42 0 -9 7 liquid_light_oil 50000 38174
42 0 -9 9 gas_natural_gas 969349 1120249
42 0 -6 -7 liquid_light_oil 50000 49986
42 0 -6 -3 liquid_light_oil 50000 48952
42 0 -6 1 gas_natural_gas 113958 107128
42 0 -6 5 gas_natural_gas 113958 132082
42 0 -6 9 liquid_heavy_oil 1331607 1652958
42 0 -3 -7 liquid_light_oil 50000 61234
42 0 -3 -3 liquid_light_oil 50000 39456
42 0 -3 1 gas_natural_gas 113958 131918
42 0 -3 3 gas_natural_gas 113958 132515
42 0 -3 5 gas_natural_gas 113958 117068
42 0 -3 9 liquid_heavy_oil 1331607 1170615
42 0 0 -7 gas_natural_gas 50000 40385
42 0 0 -3 gas_natural_gas 50000 44065
42 0 0 1 oil 126187 107767
42 0 0 5 oil 126187 140722
42 0 0 9 liquid_light_oil 50000 57223
42 0 3 -7 gas_natural_gas 50000 46885
42 0 3 -3 gas_natural_gas 50000 54102
42 0 3 -1 gas_natural_gas 50000 55118
42 0 3 1 oil 126187 144343
42 0 3 5 oil 126187 114512
42 0 3 9 liquid_light_oil 50000 41729
42 0 6 -7 gas_natural_gas 50000 54880
42 0 6 -3 gas_natural_gas 50000 39087
42 0 6 1 oil 126187 115651
42 0 6 5 oil 126187 143279
42 0 6 9 liquid_light_oil 50000 47333
42 0 9 -7 gas_natural_gas 1094826 1326465
42 0 9 -5 gas_natural_gas 1094826 848975
42 0 9 -3 gas_natural_gas 1094826 1125793
42 0 9 1 liquid_light_oil 1004477 1036574
42 0 9 5 liquid_light_oil 1004477 1059812
42 0 9 9 oil 83346 93377
42 0 -431957 -970629 liquid_medium_oil 2 2
42 0 543194 -554235 gas_natural_gas 50000 41831
42 0 -108972 -93172 liquid_heavy_oil 67699 53609
42 0 24618 -663696 oil 0 0
42 0 -322353 118555 gas_natural_gas 50000 51601
42 0 936589 666787 liquid_heavy_oil 2669180 3217926
42 0 -641956 642438 liquid_light_oil 1442862 1151775
42 0 -745474 -499628 gas_natural_gas 50000 61691
42 0 588541 -837220 liquid_medium_oil 38 46
42 0 -886935 486724 gas_natural_gas 50000 45152
42 0 554467 641416 liquid_heavy_oil 167380 170259
42 7 -9 7 gas_natural_gas 50000 51034
42 7 -6 5 liquid_medium_oil 714487 616874
42 7 -3 3 liquid_medium_oil 714487 618532
42 7 0 1 gas_natural_gas 50000 55898
42 7 3 -1 gas_natural_gas 729997 640327
42 7 6 -3 gas_natural_gas 729997 688467
42 7 9 -5 oil 2817829 2610141
42 7 -983291 106138 liquid_medium_oil 8261 6965
42 7 496325 407050 liquid_medium_oil 373761 372565
42 7 815627 -458725 gas_natural_gas 597449 686226
42 -1 -9 7 liquid_light_oil 812327 762568
42 -1 -6 5 gas_natural_gas 321170 401251
42 -1 -3 3 gas_natural_gas 321170 321130
42 -1 0 1 liquid_light_oil 50000 55105
42 -1 3 -1 oil 594593 707918
42 -1 6 -3 oil 594593 683048
42 -1 9 -5 liquid_light_oil 50000 41514
42 -1 -75463 972569 gas_natural_gas 50000 58031
42 -1 -884705 230983 liquid_medium_oil 1706191 1606082
42 -1 891406 -921257 gas_natural_gas 293909 314226
-4402670435541626946 0 -9 -7 gas_natural_gas 512603 505792
-4402670435541626946 0 -9 -3 gas_natural_gas 512603 441430
-4402670435541626946 0 -9 1 liquid_medium_oil 2725970 3004989
-4402670435541626946 0 -9 5 liquid_medium_oil 2725970 2583060
-4402670435541626946 0 -9 7 liquid_medium_oil 2725970 3145115
-4402670435541626946 0 -9 9 oil 175 135
-4402670435541626946 0 -6 -7 gas_natural_gas 50000 40524
-4402670435541626946 0 -6 -3 gas_natural_gas 50000 58166
-4402670435541626946 0 -6 1 liquid_heavy_oil 923692 776272
-4402670435541626946 0 -6 5 liquid_heavy_oil 923692 711837
-4402670435541626946 0 -6 9 oil 0 0
-4402670435541626946 0 -3 -7 gas_natural_gas 50000 40607
-4402670435541626946 0 -3 -3 gas_natural_gas 50000 60401
-4402670435541626946 0 -3 1 liquid_heavy_oil 923692 989835
-4402670435541626946 0 -3 3 liquid_heavy_oil 923692 1006220
-4402670435541626946 0 -3 5 liquid_heavy_oil 923692 839674
-4402670435541626946 0 -3 9 oil 0 0
-4402670435541626946 0 0 -7 liquid_light_oil 377767 435526
-4402670435541626946 0 0 -3 liquid_light_oil 377767 437598
-4402670435541626946 0 0 1 liquid_heavy_oil 869756 861048
-4402670435541626946 0 0 5 liquid_heavy_oil 869756 1068630
-4402670435541626946 0 0 9 gas_natural_gas 50000 59848
-4402670435541626946 0 3 -7 liquid_light_oil 377767 287878
-4402670435541626946 0 3 -3 liquid_light_oil 377767 318925
-4402670435541626946 0 3 -1 liquid_light_oil 377767 398560
-4402670435541626946 0 3 1 liquid_heavy_oil 869756 1044330
-4402670435541626946 0 3 5 liquid_heavy_oil 869756 881208
-4402670435541626946 0 3 9 gas_natural_gas 50000 61331
-4402670435541626946 0 6 -7 liquid_light_oil 377767 453444
-4402670435541626946 0 6 -3 liquid_light_oil 377767 291339
-4402670435541626946 0 6 1 liquid_heavy_oil 869756 939461
-4402670435541626946 0 6 5 liquid_heavy_oil 869756 688334
-4402670435541626946 0 6 9 gas_natural_gas 50000 49204
-4402670435541626946 0 9 -7 oil 1999 2319
-4402670435541626946 0 9 -5 oil 1999 2283
-4402670435541626946 0 9 -3 oil 1999 2075
-4402670435541626946 0 9 1 oil 1193786 1017923
-4402670435541626946 0 9 5 oil 1193786 1056910
-4402670435541626946 0 9 9 oil 2 2
-4402670435541626946 0 335388 589657 oil 39 45
-4402670435541626946 0 -738116 -766776 oil 606826 742153
-4402670435541626946 0 116437 218263 liquid_light_oil 691121 773096
-4402670435541626946 0 945102 -544709 oil 2953277 2534183
-4402670435541626946 0 344824 -651822 liquid_heavy_oil 12441 13610
-4402670435541626946 0 214223 287226 liquid_medium_oil 1776084 1475019
-4402670435541626946 0 128155 -31520 liquid_light_oil 50000 54460
-4402670435541626946 0 696476 326452 gas_natural_gas 697969 654530
-4402670435541626946 0 -765753 37225 liquid_medium_oil 222245 209052
-4402670435541626946 0 148159 -325112 liquid_medium_oil 44131 53996
-4402670435541626946 0 542463 49867 oil 123018 127582
-4402670435541626946 7 -9 7 oil 174854 169201
-4402670435541626946 7 -6 5 liquid_heavy_oil 316095 267116
-4402670435541626946 7 -3 3 liquid_heavy_oil 316095 346913
-4402670435541626946 7 0 1 gas_natural_gas 50000 50204
-4402670435541626946 7 3 -1 liquid_light_oil 55663 68786
-4402670435541626946 7 6 -3 liquid_light_oil 55663 65103
-4402670435541626946 7 9 -5 liquid_light_oil 235422 240806
-4402670435541626946 7 -100852 665212 liquid_light_oil 50000 56830
-4402670435541626946 7 670881 418386 oil 11690 10699
-4402670435541626946 7 405024 -924953 gas_natural_gas 90705 104390
-4402670435541626946 -1 -9 7 oil 22203 25765
-4402670435541626946 -1 -6 5 liquid_light_oil 1175839 1460581
-4402670435541626946 -1 -3 3 liquid_light_oil 1175839 1177885
-4402670435541626946 -1 0 1 liquid_medium_oil 2725970 3385266
-4402670435541626946 -1 3 -1 gas_natural_gas 512603 586509
-4402670435541626946 -1 6 -3 gas_natural_gas 512603 599412
-4402670435541626946 -1 9 -5 gas_natural_gas 50000 44264
-4402670435541626946 -1 -64462 90025 liquid_heavy_oil 553800 566894
-4402670435541626946 -1 426046 669265 oil 155395 146611
-4402670435541626946 -1 532376 864830 liquid_heavy_oil 23689 22335
9223372036854775807 0 -9 -7 oil 1668077 1507963
9223372036854775807 0 -9 -3 oil 1668077 1798159
9223372036854775807 0 -9 1 liquid_medium_oil 105939 120027
9223372036854775807 0 -9 5 liquid_medium_oil 105939 97725
9223372036854775807 0 -9 7 liquid_medium_oil 105939 101756
9223372036854775807 0 -9 9 liquid_medium_oil 93581 80595
9223372036854775807 0 -6 -7 gas_natural_gas 569345 682899
9223372036854775807 0 -6 -3 gas_natural_gas 569345 539752
9223372036854775807 0 -6 1 liquid_light_oil 50000 61061
9223372036854775807 0 -6 5 liquid_light_oil 50000 57549
9223372036854775807 0 -6 9 gas_natural_gas 1195259 1123984
9223372036854775807 0 -3 -7 gas_natural_gas 569345 602052
9223372036854775807 0 -3 -3 gas_natural_gas 569345 703723
9223372036854775807 0 -3 1 liquid_light_oil 50000 62361
9223372036854775807 0 -3 3 liquid_light_oil 50000 56290
9223372036854775807 0 -3 5 liquid_light_oil 50000 38090
9223372036854775807 0 -3 9 gas_natural_gas 1195259 1329349
9223372036854775807 0 0 -7 oil 1332409 1266234
9223372036854775807 0 0 -3 oil 1332409 1368755
9223372036854775807 0 0 1 gas_natural_gas 71256 73499
9223372036854775807 0 0 5 gas_natural_gas 71256 72381
9223372036854775807 0 0 9 liquid_light_oil 50000 61857
9223372036854775807 0 3 -7 oil 1332409 1594425
9223372036854775807 0 3 -3 oil 1332409 1117820
9223372036854775807 0 3 -1 oil 1332409 1398847
9223372036854775807 0 3 1 gas_natural_gas 71256 87073
9223372036854775807 0 3 5 gas_natural_gas 71256 54357
9223372036854775807 0 3 9 liquid_light_oil 50000 49183
9223372036854775807 0 6 -7 oil 1332409 999602
9223372036854775807 0 6 -3 oil 1332409 1089885
9223372036854775807 0 6 1 gas_natural_gas 71256 56888
9223372036854775807 0 6 5 gas_natural_gas 71256 74849
9223372036854775807 0 6 9 liquid_light_oil 50000 62041
9223372036854775807 0 9 -7 gas_natural_gas 50000 49977
9223372036854775807 0 9 -5 gas_natural_gas 50000 49473
9223372036854775807 0 9 -3 gas_natural_gas 50000 43537
9223372036854775807 0 9 1 oil 792765 623253
9223372036854775807 0 9 5 oil 792765 779228
9223372036854775807 0 9 9 oil 44037 34101
9223372036854775807 0 27456 -683835 oil 388 345
9223372036854775807 0 -178615 182012 gas_natural_gas 87878 73472
9223372036854775807 0 475211 -294507 liquid_heavy_oil 19743 22639
9223372036854775807 0 894378 933029 liquid_light_oil 50000 37918
9223372036854775807 0 -274750 -254589 liquid_heavy_oil 26015 21273
9223372036854775807 0 867813 -714101 liquid_heavy_oil 0 0
9223372036854775807 0 986412 350025 gas_natural_gas 50000 52012
9223372036854775807 0 40496 813176 liquid_heavy_oil 478 510
9223372036854775807 0 -193465 -753608 gas_natural_gas 50000 62252
9223372036854775807 0 -901173 -467268 liquid_medium_oil 1417589 1552633
9223372036854775807 0 -43587 -263356 oil 1728835 1615800
9223372036854775807 7 -9 7 liquid_light_oil 50000 44511
9223372036854775807 7 -6 5 gas_natural_gas 184095 203654
9223372036854775807 7 -3 3 gas_natural_gas 184095 221457
9223372036854775807 7 0 1 liquid_medium_oil 239414 244042
9223372036854775807 7 3 -1 liquid_heavy_oil 856393 941323
9223372036854775807 7 6 -3 liquid_heavy_oil 856393 765922
9223372036854775807 7 9 -5 oil 2843249 3423935
9223372036854775807 7 933554 -55108 liquid_light_oil 773655 782201
9223372036854775807 7 325426 559588 oil 564131 650952
9223372036854775807 7 16331 -905841 gas_natural_gas 658136 744108
9223372036854775807 -1 -9 7 liquid_heavy_oil 1208572 1188104
9223372036854775807 -1 -6 5 gas_natural_gas 1141577 985681
9223372036854775807 -1 -3 3 gas_natural_gas 1141577 919303
9223372036854775807 -1 0 1 liquid_medium_oil 105939 82988
9223372036854775807 -1 3 -1 oil 1668077 2031161
9223372036854775807 -1 6 -3 oil 1668077 1624838
9223372036854775807 -1 9 -5 gas_natural_gas 569345 701854
9223372036854775807 -1 -745071 681144 oil 0 0
9223372036854775807 -1 172250 -201130 oil 265835 208588
9223372036854775807 -1 646571 737432 gas_natural_gas 89438 109984
-9223372036854775808 0 -9 -7 gas_natural_gas 569345 572377
-9223372036854775808 0 -9 -3 gas_natural_gas 569345 586723
-9223372036854775808 0 -9 1 liquid_light_oil 50000 56747
-9223372036854775808 0 -9 5 liquid_light_oil 50000 47731
-9223372036854775808 0 -9 7 liquid_light_oil 50000 38598
-9223372036854775808 0 -9 9 gas_natural_gas 1195259 1467401
-9223372036854775808 0 -6 -7 oil 1332409 1003541
-9223372036854775808 0 -6 -3 oil 1332409 1157395
-9223372036854775808 0 -6 1 gas_natural_gas 71256 67820
-9223372036854775808 0 -6 5 gas_natural_gas 71256 67637
-9223372036854775808 0 -6 9 liquid_light_oil 50000 59885
-9223372036854775808 0 -3 -7 oil 1332409 1590873
-9223372036854775808 0 -3 -3 oil 1332409 1394545
-9223372036854775808 0 -3 1 gas_natural_gas 71256 67173
-9223372036854775808 0 -3 3 gas_natural_gas 71256 58439
-9223372036854775808 0 -3 5 gas_natural_gas 71256 58714
-9223372036854775808 0 -3 9 liquid_light_oil 50000 41865
-9223372036854775808 0 0 -7 gas_natural_gas 50000 47567
-9223372036854775808 0 0 -3 gas_natural_gas 50000 51366
-9223372036854775808 0 0 1 oil 792765 616844
-9223372036854775808 0 0 5 oil 792765 780608
-9223372036854775808 0 0 9 oil 44037 54438
-9223372036854775808 0 3 -7 gas_natural_gas 50000 47210
-9223372036854775808 0 3 -3 gas_natural_gas 50000 55981
-9223372036854775808 0 3 -1 gas_natural_gas 50000 61834
-9223372036854775808 0 3 1 oil 792765 949815
-9223372036854775808 0 3 5 oil 792765 934262
-9223372036854775808 0 3 9 oil 44037 53704
-9223372036854775808 0 6 -7 gas_natural_gas 50000 41907
-9223372036854775808 0 6 -3 gas_natural_gas 50000 61434
-9223372036854775808 0 6 1 oil 792765 914583
-9223372036854775808 0 6 5 oil 792765 875855
-9223372036854775808 0 6 9 oil 44037 51472
-9223372036854775808 0 9 -7 liquid_light_oil 50000 49930
-9223372036854775808 0 9 -5 liquid_light_oil 50000 59407
-9223372036854775808 0 9 -3 liquid_light_oil 50000 46672
-9223372036854775808 0 9 1 liquid_light_oil 1236172 975026
-9223372036854775808 0 9 5 liquid_light_oil 1236172 1132330
-9223372036854775808 0 9 9 liquid_medium_oil 892231 814336
-9223372036854775808 0 -144469 -112539 oil 2995 3326
-9223372036854775808 0 916152 670764 liquid_light_oil 50000 62159
-9223372036854775808 0 -953280 -254998 liquid_light_oil 50000 50015
-9223372036854775808 0 276046 186073 oil 1177670 1438610
-9223372036854775808 0 -811028 768785 liquid_medium_oil 1175 1147
-9223372036854775808 0 3436 -26197 oil 74002 78877
-9223372036854775808 0 -900045 -535733 gas_natural_gas 59725 62461
-9223372036854775808 0 737905 -238528 gas_natural_gas 53838 43823
-9223372036854775808 0 -373515 -820304 liquid_heavy_oil 22802 19831
-9223372036854775808 0 -310663 -455554 liquid_medium_oil 491404 556870
-9223372036854775808 0 -810963 762232 oil 2523720 2252092
-9223372036854775808 7 -9 7 gas_natural_gas 184095 222710
-9223372036854775808 7 -6 5 liquid_medium_oil 239414 193976
-9223372036854775808 7 -3 3 liquid_medium_oil 239414 181626
-9223372036854775808 7 0 1 liquid_heavy_oil 428 328
-9223372036854775808 7 3 -1 oil 2843249 3301008
-9223372036854775808 7 6 -3 oil 2843249 3010914
-9223372036854775808 7 9 -5 oil 661995 656699
-9223372036854775808 7 -42660 -433739 liquid_medium_oil 1860760 2099784
-9223372036854775808 7 703889 -882371 gas_natural_gas 389936 482917
-9223372036854775808 7 -435123 146733 liquid_medium_oil 875937 1088131
-9223372036854775808 -1 -9 7 gas_natural_gas 1141577 1346683
-9223372036854775808 -1 -6 5 liquid_medium_oil 105939 109796
-9223372036854775808 -1 -3 3 liquid_medium_oil 105939 132382
-9223372036854775808 -1 0 1 liquid_light_oil 50000 51719
-9223372036854775808 -1 3 -1 gas_natural_gas 569345 586968
-9223372036854775808 -1 6 -3 gas_natural_gas 569345 610915
-9223372036854775808 -1 9 -5 oil 1332409 1260513
-9223372036854775808 -1 -902584 -625128 liquid_light_oil 50000 46861
-9223372036854775808 -1 49484 -216774 gas_natural_gas 50000 58016
-9223372036854775808 -1 362609 811733 liquid_heavy_oil 169979 174403
-24806 0 -9 -7 liquid_heavy_oil 9869 8467
-24806 0 -9 -3 liquid_heavy_oil 9869 10001
-24806 0 -9 1 liquid_medium_oil 2928899 3176336
-24806 0 -9 5 liquid_medium_oil 2928899 3164885
-24806 0 -9 7 liquid_medium_oil 2928899 2817910
-24806 0 -9 9 oil 2184326 2120668
-24806 0 -6 -7 oil 353669 325868
-24806 0 -6 -3 oil 353669 320540
-24806 0 -6 1 oil 0 0
-24806 0 -6 5 oil 0 0
-24806 0 -6 9 liquid_medium_oil 1504710 1479984
-24806 0 -3 -7 oil 353669 413253
-24806 0 -3 -3 oil 353669 325873
-24806 0 -3 1 oil 0 0
-24806 0 -3 3 oil 0 0
-24806 0 -3 5 oil 0 0
-24806 0 -3 9 liquid_medium_oil 1504710 1402921
-24806 0 0 -7 liquid_light_oil 778856 638920
-24806 0 0 -3 liquid_light_oil 778856 857696
-24806 0 0 1 liquid_light_oil 300102 318059
-24806 0 0 5 liquid_light_oil 300102 368289
-24806 0 0 9 liquid_heavy_oil 1790956 1631047
-24806 0 3 -7 liquid_light_oil 778856 910600
-24806 0 3 -3 liquid_light_oil 778856 618317
-24806 0 3 -1 liquid_light_oil 778856 754761
-24806 0 3 1 liquid_light_oil 300102 323927
-24806 0 3 5 liquid_light_oil 300102 361161
-24806 0 3 9 liquid_heavy_oil 1790956 2235081
-24806 0 6 -7 liquid_light_oil 778856 585266
-24806 0 6 -3 liquid_light_oil 778856 817284
-24806 0 6 1 liquid_light_oil 300102 282984
-24806 0 6 5 liquid_light_oil 300102 250078
-24806 0 6 9 liquid_heavy_oil 1790956 2206662
-24806 0 9 -7 oil 193088 231240
-24806 0 9 -5 oil 193088 166137
-24806 0 9 -3 oil 193088 201879
-24806 0 9 1 liquid_medium_oil 2756894 2561446
-24806 0 9 5 liquid_medium_oil 2756894 3284500
-24806 0 9 9 gas_natural_gas 50000 53491
-24806 0 567454 -328383 liquid_heavy_oil 234212 253081
-24806 0 -548725 -493847 liquid_heavy_oil 1147451 1051396
-24806 0 -840405 -374116 oil 59922 70119
-24806 0 -726703 56899 gas_natural_gas 50000 54614
-24806 0 -543516 -173120 liquid_medium_oil 447 410
-24806 0 -395205 -322716 liquid_medium_oil 87302 70485
-24806 0 245790 685295 oil 457 424
-24806 0 112128 199758 liquid_medium_oil 15422 12991
-24806 0 294486 460729 oil 443930 375985
-24806 0 995554 219470 liquid_medium_oil 130057 103408
-24806 0 436392 437871 liquid_medium_oil 0 0
-24806 7 -9 7 liquid_heavy_oil 2165 2286
-24806 7 -6 5 liquid_medium_oil 0 0
-24806 7 -3 3 liquid_medium_oil 0 0
-24806 7 0 1 liquid_heavy_oil 77313 64001
-24806 7 3 -1 gas_natural_gas 50000 49724
-24806 7 6 -3 gas_natural_gas 50000 55919
-24806 7 9 -5 liquid_light_oil 50000 48417
-24806 7 178645 295269 liquid_medium_oil 407611 453981
-24806 7 238547 739530 gas_natural_gas 50000 56504
-24806 7 900332 834846 liquid_light_oil 50000 48587
-24806 -1 -9 7 oil 1182528 911890
-24806 -1 -6 5 gas_natural_gas 50000 41858
-24806 -1 -3 3 gas_natural_gas 50000 61756
-24806 -1 0 1 liquid_medium_oil 2928899 2377808
-24806 -1 3 -1 liquid_heavy_oil 9869 8131
-24806 -1 6 -3 liquid_heavy_oil 9869 8734
-24806 -1 9 -5 oil 353669 405480
-24806 -1 42098 -544515 liquid_light_oil 215050 198221
-24806 -1 -407513 70546 oil 372604 439395
-24806 -1 -242463 -975001 gas_natural_gas 185821 206641
1337 0 -8 8 liquid_medium_oil 392932 444205
1337 0 -8 9 liquid_medium_oil 392932 336738
1337 0 -8 10 liquid_medium_oil 392932 325815
1337 0 -8 11 liquid_medium_oil 392932 429716
1337 0 -8 12 liquid_medium_oil 392932 382448
1337 0 -8 13 liquid_medium_oil 392932 365836
1337 0 -8 14 liquid_medium_oil 392932 382309
1337 0 -8 15 liquid_medium_oil 392932 488795
1337 0 -7 8 liquid_medium_oil 392932 312485
1337 0 -7 9 liquid_medium_oil 392932 386158
1337 0 -7 10 liquid_medium_oil 392932 332953
1337 0 -7 11 liquid_medium_oil 392932 471618
1337 0 -7 12 liquid_medium_oil 392932 490773
1337 0 -7 13 liquid_medium_oil 392932 397511
1337 0 -7 14 liquid_medium_oil 392932 476813
1337 0 -7 15 liquid_medium_oil 392932 374443
1337 0 -6 8 liquid_medium_oil 392932 349570
1337 0 -6 9 liquid_medium_oil 392932 486147
1337 0 -6 10 liquid_medium_oil 392932 405574
1337 0 -6 11 liquid_medium_oil 392932 356169
1337 0 -6 12 liquid_medium_oil 392932 490064
1337 0 -6 13 liquid_medium_oil 392932 295608
1337 0 -6 14 liquid_medium_oil 392932 447247
1337 0 -6 15 liquid_medium_oil 392932 313122
1337 0 -5 8 liquid_medium_oil 392932 320013
1337 0 -5 9 liquid_medium_oil 392932 485933
1337 0 -5 10 liquid_medium_oil 392932 329201
1337 0 -5 11 liquid_medium_oil 392932 313429
1337 0 -5 12 liquid_medium_oil 392932 472512
1337 0 -5 13 liquid_medium_oil 392932 382611
1337 0 -5 14 liquid_medium_oil 392932 398069
1337 0 -5 15 liquid_medium_oil 392932 402173
1337 0 -4 8 liquid_medium_oil 392932 321037
1337 0 -4 9 liquid_medium_oil 392932 431954
1337 0 -4 10 liquid_medium_oil 392932 487729
1337 0 -4 11 liquid_medium_oil 392932 474170
1337 0 -4 12 liquid_medium_oil 392932 449354
1337 0 -4 13 liquid_medium_oil 392932 385258
1337 0 -4 14 liquid_medium_oil 392932 411063
1337 0 -4 15 liquid_medium_oil 392932 296208
1337 0 -3 8 liquid_medium_oil 392932 421921
1337 0 -3 9 liquid_medium_oil 392932 390088
1337 0 -3 10 liquid_medium_oil 392932 445559
1337 0 -3 11 liquid_medium_oil 392932 318833
1337 0 -3 12 liquid_medium_oil 392932 459736
1337 0 -3 13 liquid_medium_oil 392932 445072
1337 0 -3 14 liquid_medium_oil 392932 442620
1337 0 -3 15 liquid_medium_oil 392932 365134
1337 0 -2 8 liquid_medium_oil 392932 486109
1337 0 -2 9 liquid_medium_oil 392932 352458
1337 0 -2 10 liquid_medium_oil 392932 488656
1337 0 -2 11 liquid_medium_oil 392932 379778
1337 0 -2 12 liquid_medium_oil 392932 356389
1337 0 -2 13 liquid_medium_oil 392932 453959
1337 0 -2 14 liquid_medium_oil 392932 328682
1337 0 -2 15 liquid_medium_oil 392932 480304
1337 0 -1 8 liquid_medium_oil 392932 304276
1337 0 -1 9 liquid_medium_oil 392932 434243
1337 0 -1 10 liquid_medium_oil 392932 400045
1337 0 -1 11 liquid_medium_oil 392932 457226
1337 0 -1 12 liquid_medium_oil 392932 398132
1337 0 -1 13 liquid_medium_oil 392932 443067
1337 0 -1 14 liquid_medium_oil 392932 470618
1337 0 -1 15 liquid_medium_oil 392932 385576
42 0 0 0 oil 126187 145304
";

/// `Math.pow(amount * 100.0 * 5000, 0.2)` bits (HotSpot; StrictMath agrees on all but 10000, see below).
const POW: &[(i32, u64)] = &[
    (0, 0),
    (4, 0x403234a4f4c2a738),
    (10, 0x4035de03899d7658),
    (200, 0x4043e7c5939384ad),
    (350, 0x4046433f9178c7b0),
    (625, 0x4049000000000002),
    (800, 0x404a43e9430b7d1a),
    (1600, 0x404e2bbee6e6caf9),
];

fn table() -> FluidTable {
    FluidTable::overworld_default()
}

#[test]
fn guava17_iteration_order() {
    let t = table();
    let regs: Vec<&str> = t.fluids.iter().map(|f| f.registry.as_str()).collect();
    assert_eq!(
        regs,
        [
            "liquid_light_oil",
            "liquid_medium_oil",
            "oil",
            "gas_natural_gas",
            "liquid_heavy_oil"
        ]
    );
    assert_eq!(t.max_chance, 100);
    assert_eq!(java_string_hash("oil"), 110_034);
}

#[test]
fn pow_matches_hotspot() {
    for &(amt, bits) in POW {
        let f = UoFluid::new("x", "x", amt, amt.max(1), 1, 5);
        assert_eq!(f.smin.to_bits(), bits, "smin {amt}");
    }
    // smax = floor(pow(max*5e5, .2)): 350 → 44, 625 → 50 (50^5 = 3.125e8 exactly; pow lands 2 ulp above)
    assert_eq!(UoFluid::new("x", "x", 0, 350, 1, 5).smax, 44);
    assert_eq!(UoFluid::new("x", "x", 0, 625, 1, 5).smax, 50);
}

#[test]
fn xstr_matches_gtores() {
    for s in [0i64, 1, -1, 1337, i64::MIN] {
        let (mut a, mut b) = (Xstr::new(s), rwg2_biomes::gtores::Xstr::new(s));
        for _ in 0..100 {
            assert_eq!(a.next_int(1000), b.next_int(1000));
        }
    }
}

#[test]
fn pristine_matches_java() {
    let t = table();
    let mut n = 0;
    for l in GOLDEN.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = l.split(' ').collect();
        let (seed, dim, cx, cz): (i64, i32, i32, i32) = (
            f[0].parse().unwrap(),
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        );
        let s = fluid_at(seed, dim, cx, cz, &t).expect(l);
        let got = format!(
            "{} {} {}",
            t.fluids[s.fluid_index].registry, s.vein_average, s.amount
        );
        assert_eq!(got, f[4..].join(" "), "{l}");
        assert_eq!((s.region_x, s.region_z), (cx >> 3, cz >> 3));
        // field_chunks agrees with fluid_at
        let (fs, a) = field_chunks(seed, dim, cx >> 3, cz >> 3, &t).unwrap();
        assert_eq!(fs.fluid_index, s.fluid_index);
        assert_eq!(a[(((cx & 7) << 3) | (cz & 7)) as usize], s.amount);
        n += 1;
    }
    assert!(n > 700);
}

#[test]
fn hand_checks() {
    let t = table();
    // XSTR seed 0 → 42: seed 8266 + rx 1 + 8267 * rz(−1) == 0
    assert_eq!(field_seed(8266, 0, 1, -1), 0);
    let a = fluid_at(8266, 0, 8, -8, &t).unwrap();
    let b = fluid_at(42, 0, 0, 0, &t).unwrap();
    assert_eq!(
        (a.fluid_index, a.vein_average, a.amount),
        (b.fluid_index, b.vein_average, b.amount)
    );
    // min clamp: natural gas / light oil min 10 → smin^5/100 == 50000 on HotSpot
    let mut clamped = 0;
    for rx in 0..2000 {
        if let Some(f) = field_at(1, 0, rx, 0, &t) {
            let fl = &t.fluids[f.fluid_index];
            assert!(f.vein_average >= fl.min_amount * DIVIDER, "{f:?}");
            assert!(f.vein_average < fl.max_amount * DIVIDER, "{f:?}");
            clamped += (f.vein_average == fl.min_amount * DIVIDER && fl.min_amount > 0) as i32;
        }
    }
    assert!(clamped > 0);
    // getRandomFluid with `<=`: r ∈ [0,200] → first, (200,400] → second, …, (800,999] → fifth; never None here
    for s in 0..5000i64 {
        assert!(field_at(s, 0, 0, 0, &t).is_some());
    }
}

#[test]
fn per_op_and_geometry() {
    let s = FluidSample {
        fluid_index: 0,
        amount: 5000,
        vein_average: 0,
        region_x: 0,
        region_z: 0,
    };
    assert_eq!(s.litres_per_op(), 0);
    assert_eq!(
        FluidSample {
            amount: 10_001,
            ..s
        }
        .litres_per_op(),
        2
    );
    let v: Vec<_> = fields_in_rect(-1, -1, 128, 0).collect();
    assert_eq!(v, [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0)]);
    assert_eq!(field_dist2(0, 0, 0, 0), 0);
    assert_eq!(field_dist2(-1, 0, 0, 0), 1);
    assert_eq!(field_dist2(130, 0, 0, 0), 9);
    let w = fields_within(64, 64, 64);
    assert_eq!(w[0], (0, 0));
    assert_eq!(w, [(0, 0), (1, 0), (0, 1)]); // block 128 is 64 away, block −1 is 65
    let t = table();
    let hit = nearest_field(1337, 0, &t, 0, 0, 2000, |f| {
        t.fluids[f.fluid_index].registry == "oil"
    })
    .unwrap();
    assert_eq!(t.fluids[hit.fluid_index].registry, "oil");
}

#[cfg(feature = "json")]
#[test]
fn json_table_matches_default() {
    let root = std::env::var("RWG2_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
        });
    let s = std::fs::read_to_string(root.join("gtnh-2.9.0/gtfluids.json")).unwrap();
    let j = FluidTable::from_json(&s, "0").unwrap();
    let d = table();
    assert!(
        !j.provisional,
        "confirmed by rwg2dump 2.6 (live table + 2401 chunk amounts)"
    );
    assert_eq!(j.max_chance, d.max_chance);
    for (a, b) in j.fluids.iter().zip(&d.fluids) {
        assert_eq!(
            (&a.registry, a.min_amount, a.max_amount, a.chance),
            (&b.registry, b.min_amount, b.max_amount, b.chance)
        );
    }
    assert_eq!(j.fluids.len(), d.fluids.len());
}

/// Pristine amounts from the live game (rwg2dump 2.6, seed 1337, chunks [-24, 24]^2, GT's own getPristineAmount).
#[cfg(feature = "json")]
#[test]
fn matches_pack_dump26() {
    let root = std::env::var("RWG2_DATA_DIR")
        .map(|d| std::path::PathBuf::from(d).parent().unwrap().to_path_buf())
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let Ok(s) = std::fs::read_to_string(root.join("testdata/pack/gtnh-2.9.0/dump26/gtfluids.json"))
    else {
        eprintln!("skip");
        return;
    };
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let t = rwg2_biomes::gtfluids::FluidTable::overworld_default();
    // live HashBiMap order = our pick order
    let live: Vec<&str> = v["table"]["fluids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["registry"].as_str().unwrap())
        .collect();
    let ours: Vec<&str> = t.fluids.iter().map(|f| f.registry.as_str()).collect();
    assert_eq!(live, ours);
    let mut n = 0;
    for c in v["chunks"].as_array().unwrap() {
        let (cx, cz) = (c[0].as_i64().unwrap() as i32, c[1].as_i64().unwrap() as i32);
        let got = rwg2_biomes::gtfluids::fluid_at(1337, 0, cx, cz, &t);
        let want_reg = c[2].as_str();
        let want_amt = c[3].as_i64().unwrap() as i32;
        let g = got.map(|g| (t.fluids[g.fluid_index].registry.as_str(), g.amount));
        assert_eq!(g, want_reg.map(|r| (r, want_amt)), "chunk {cx},{cz}");
        n += 1;
    }
    assert_eq!(n, 49 * 49);
}
