package net.minecraft.world.gen.structure; import net.minecraft.world.ChunkCoordIntPair;
public class MapGenStronghold extends MapGenStructure { private ChunkCoordIntPair[] field_75057_g = new ChunkCoordIntPair[3]; private boolean ran;
  protected boolean func_75047_a(int x,int z){ if(!ran){ ran=true; field_75057_g[0]=new ChunkCoordIntPair(40,-12); field_75057_g[1]=new ChunkCoordIntPair(-60,30); field_75057_g[2]=new ChunkCoordIntPair(10,70);} for(ChunkCoordIntPair p:field_75057_g) if(p.field_77276_a==x&&p.field_77275_b==z) return true; return false; } }
