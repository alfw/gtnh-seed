package rwg2.world; import java.util.*; import rwg2.biomes.realistic.RealisticBiomeBase; import net.minecraft.world.biome.BiomeGenBase; import rwg2.support.Support;
public class ChunkManagerRealistic extends WorldChunkManager { private rwg2.util.ContinentalNoise continents = new rwg2.util.ContinentalNoise(); private final long seed; private boolean continental; private float climateWidth=1400f;
  private ArrayList<RealisticBiomeBase> biomes_snow = new ArrayList<>(Arrays.asList(RealisticBiomeBase.A, RealisticBiomeBase.B));
  private ArrayList<RealisticBiomeBase>[] borderBiomes = new ArrayList[]{ new ArrayList<>(Arrays.asList(RealisticBiomeBase.C)), new ArrayList<>(), new ArrayList<>(), new ArrayList<>()};
  public ChunkManagerRealistic(long seed, boolean c){this.seed=seed; continental=c;}
  private int h(int x,int z){ long v=(x*31L+z)*0x9E3779B97F4A7C15L ^ seed ^ (continental?7:0); return (int)((v>>>40)%3); }
  public RealisticBiomeBase getBiomeDataAt(int x,int z){ return h(x,z)==0?RealisticBiomeBase.A:h(x,z)==1?RealisticBiomeBase.B:RealisticBiomeBase.C; }
  public BiomeGenBase func_76935_a(int x,int z){ return (x==0&&z==0&&continental)?Support.lavaCaveMarkerBiome:getBiomeDataAt(x,z).baseBiome; }
  public BiomeGenBase[] func_76933_b(BiomeGenBase[] a,int x,int z,int w,int h){return a;}
  int getMetaBiomeAt(int x,int z){return 1+h(x,z);}
  public float getContinentValue(int x,int z){return continental? x*0.5f : -1f;}
  public float getNoiseAt(int x,int z){return 63f+h(x,z);}
  public long getVolcanoCoordinates(int x,int z){return continental?1L:Long.MIN_VALUE;}
  public float getVolcanoBaseHeight(int x,int z){return 71.5f;}
  public long getLavaCaveCoordinates(int x,int z){return Long.MIN_VALUE;}
  List<RealisticBiomeBase> getConfiguredBiomes(){ return Arrays.asList(RealisticBiomeBase.A,RealisticBiomeBase.B,RealisticBiomeBase.C);} }
