package rwg2.util;
public class ContinentalNoise {
  public static int unpackVolcanoX(long c){return (int)(c>>32);} public static int unpackVolcanoY(long c){return (int)c;}
  public long getVolcanoSeedKey(int x,int z){ return (Math.abs(x)<300&&Math.abs(z)<300)?77L:Long.MIN_VALUE; }
  public long getVolcanoCenterCoordinates(int x,int z){ return (10L<<32)|(20L&0xffffffffL); }
  public boolean isIslandVolcano(int x,int z){ return false; }
  public long getLavaCaveSeedKey(int x,int z){ return (x>1000&&x<1300&&Math.abs(z)<200)?99L:Long.MIN_VALUE; }
  public long getLavaCaveCenterCoordinates(int x,int z){ return (1100L<<32)|((-5L)&0xffffffffL); }
}
