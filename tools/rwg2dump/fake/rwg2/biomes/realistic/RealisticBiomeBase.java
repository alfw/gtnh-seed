package rwg2.biomes.realistic; import net.minecraft.world.biome.BiomeGenBase;
public class RealisticBiomeBase { private static final RealisticBiomeBase[] biomeList = new RealisticBiomeBase[256]; private static int next;
  public final int biomeID; public final int subID; public final BiomeGenBase baseBiome; public final RealisticBiomeBase beachBiome; public final BiomeGenBase riverBiome; private String displayName; private rwg2.terrain.TerrainBase terrain = new rwg2.terrain.TerrainHilly(); private boolean shallow = true;
  public RealisticBiomeBase(BiomeGenBase b){biomeID=next++; biomeList[biomeID]=this; subID=0; baseBiome=b; beachBiome=null; riverBiome=new BiomeGenBase(7,"River");}
  public String getDisplayName(){return displayName;}
  public static final RealisticBiomeBase A=new RealisticBiomeBase(new BiomeGenBase(300,"Alpha")), B=new RealisticBiomeBase(new BiomeGenBase(2,"Beta")), C=new RealisticBiomeBase(new BiomeGenBase(3,"Gamma")); }
