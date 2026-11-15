package rwg2.world; import net.minecraft.world.gen.structure.*;
public class ChunkGeneratorRealistic implements net.minecraft.world.chunk.IChunkProvider { private long worldSeed=-6999513761975938210L; private boolean continental=true;
  private final MapGenStronghold strongholdGenerator=new MapGenStronghold(); private final MapGenMineshaft mineshaftGenerator=new MapGenMineshaft(); private final MapGenVillage villageGenerator=new MapGenVillage();
  private ChunkManagerRealistic cmr; public ChunkGeneratorRealistic(){} public ChunkGeneratorRealistic(ChunkManagerRealistic m,long s,boolean c){cmr=m;worldSeed=s;continental=c;}
  private final float[] testHeight=new float[256];
  public float[] getNewNoise(ChunkManagerRealistic m,int x,int y,rwg2.biomes.realistic.RealisticBiomeBase[] b){ for(int i=0;i<16;i++)for(int j=0;j<16;j++){ b[j*16+i]=m.getBiomeDataAt(x+i,y+j); testHeight[i*16+j]=60f+i+j*0.5f;} return testHeight; } }
